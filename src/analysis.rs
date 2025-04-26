use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use walkdir::{DirEntry, WalkDir};
use std::collections::{HashSet};
use rayon::prelude::*;
use tree_sitter::{Parser, Language, Query, QueryCursor, Node};



const IGNORED_DIRS: &[&str] = &["node_modules", ".git", ".next", ".cursor", "target"];
const IGNORED_FILES: &[&str] = &["pnpm-lock.yaml", "yarn.lock", "package-lock.json"];



#[derive(Clone, Debug)]
pub struct DetectedConnection {
    pub source_file: PathBuf,
    pub imported_string: String,

}

#[derive(Clone, Debug)]
pub struct DetectedDefinition {
    pub source_file: PathBuf,
    pub symbol_name: String,
    pub kind: String, // e.g., "Function", "Class", "Const", "Let", "Var", "Export"
    pub line_number: usize, // Line number where the definition starts
}


pub type AnalysisResult = Result<(PathBuf, Vec<PathBuf>, Vec<DetectedConnection>, Vec<DetectedDefinition>), String>;

// --- Tree-sitter Languages (Extern declarations) ---
unsafe extern "C" { fn tree_sitter_javascript() -> Language; }
unsafe extern "C" { fn tree_sitter_typescript() -> Language; }
unsafe extern "C" { fn tree_sitter_tsx() -> Language; }


// --- Helper Functions (Internal) ---


fn is_ignored(entry: &DirEntry) -> bool {
    let path = entry.path();
    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
        if entry.file_type().is_dir() {
            IGNORED_DIRS.contains(&filename)
        } else {
            IGNORED_FILES.contains(&filename)
        }
    } else {
        false
    }
}


fn analyze_file_content(path: &Path) -> (Vec<DetectedConnection>, Vec<DetectedDefinition>) {
    let mut connections = Vec::new();
    let mut definitions = Vec::new();
    let file_content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return (connections, definitions),
    };

    let language_ref = match path.extension().and_then(|ext| ext.to_str()) {
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => unsafe { &tree_sitter_javascript() },
        Some("ts") => unsafe { &tree_sitter_typescript() },
        Some("tsx") => unsafe { &tree_sitter_tsx() },
        _ => return (connections, definitions),
    };

    let mut parser = Parser::new();
    if parser.set_language(language_ref).is_err() {
        eprintln!("Error setting language for file: {}", path.display());
        return (connections, definitions);
    }

    let tree = match parser.parse(&file_content, None) {
        Some(tree) => tree,
        None => {
            eprintln!("Error parsing file: {}", path.display());
            return (connections, definitions);
        }
    };

    // Define tree-sitter queries for different import types
    // Updated query for TS/TSX compatibility - Removed import_declaration attempt
    let import_query_str = r#"
        [
          ; Static ES6 Imports & Exports from '...'
          (import_statement source: (string) @import_path)
          (export_statement source: (string) @import_path)

          ; CommonJS Requires: require('...') or require`...`
          (call_expression
            function: (identifier) @require_func (#eq? @require_func "require")
            arguments: (arguments (string) @import_path))
          (call_expression
            function: (identifier) @require_func (#eq? @require_func "require")
            arguments: (arguments (template_string) @import_path))
            
          ; Dynamic Imports: import('...') or import`...`
          (call_expression
            function: (import) @import_func
            arguments: (arguments (string) @import_path))
           (call_expression
            function: (import) @import_func
            arguments: (arguments (template_string) @import_path))
            
           ; Removed: Handle potential 'import_declaration'...
           ; (import_declaration source: (string) @import_path) 
        ]
    "#;


    let query = match Query::new(language_ref, import_query_str) {
        Ok(q) => q,
        Err(e) => {
            // Print error with file path for better debugging
            eprintln!("Error creating query for {}: {:?}", path.display(), e);
            return (connections, definitions);
        }
    };

    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.matches(&query, tree.root_node(), file_content.as_bytes());

    for mat in matches {
        // Find the capture named "import_path"
        for cap in mat.captures {
             if query.capture_names()[cap.index as usize] == "import_path" {
                let node = cap.node;
                if let Some(import_path_raw) = file_content.get(node.byte_range()) {
                     // Remove quotes (single, double) or backticks
                     let import_path = import_path_raw.trim_matches(|c| c == '\'' || c == '"' || c == '`').to_string();
                     if !import_path.is_empty() {
                         connections.push(DetectedConnection {
                            source_file: path.to_path_buf(),
                            imported_string: import_path,
                        });
                     }
                 }
                break; // Found the import_path, no need to check other captures in this match
             }
         }
    }

    // --- NUEVA Consulta para Definiciones y Exportaciones ---
    let definition_query_str = r#"
        [
          ; Funciones
          (function_declaration name: (identifier) @def.name) @def.function
          (lexical_declaration
            (variable_declarator name: (identifier) @def.name value: [
              (arrow_function)
              (function_expression)
            ])
          ) @def.function.lexical
          (export_statement declaration: (function_declaration name: (identifier) @def.name)) @def.function.exported.decl

          ; Clases
          (class_declaration name: (type_identifier) @def.name) @def.class
          (export_statement declaration: (class_declaration name: (type_identifier) @def.name)) @def.class.exported.decl

          ; Variables/Constantes (exportadas o de nivel superior)
          (export_statement declaration: (lexical_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl
          (export_statement (variable_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl.var
          ; (program (lexical_declaration (variable_declarator name: (identifier) @def.name))) @def.var.toplevel ; Podría ser muy ruidoso, comentar por ahora
        ]
    "#;

    let def_query = match Query::new(language_ref, definition_query_str) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("Error creating definition query for {}: {:?}", path.display(), e);
            return (connections, definitions); // Retornar definiciones vacías también
        }
    };

    let mut def_query_cursor = QueryCursor::new();
    let def_matches = def_query_cursor.matches(&def_query, tree.root_node(), file_content.as_bytes());

    // Indices para capturas específicas (más eficiente que buscar por nombre en el bucle)
    let name_capture_index = def_query.capture_index_for_name("def.name");
    // No necesitamos el índice del nombre del patrón aquí

    for mat in def_matches {
        let mut definition_name : Option<String> = None;
        let mut kind_str : Option<String> = None;
        let mut node_for_line : Option<Node> = None; // Nodo para obtener la línea inicial

        // Iterar sobre las capturas del match actual
        for cap in mat.captures {
            let capture_index = cap.index;
            let capture_name = &def_query.capture_names()[capture_index as usize];

            // Es la captura del nombre? ("def.name")
            if Some(capture_index) == name_capture_index {
                if let Some(name_str) = file_content.get(cap.node.byte_range()) {
                    definition_name = Some(name_str.to_string());
                }
            }
            // Es una captura que define el tipo? (empieza con "def.")
            else if capture_name.starts_with("def.") {
                 kind_str = Some(match *capture_name {
                     "def.function" | "def.function.lexical" | "def.function.exported" | "def.function.exported.decl" => "Function",
                     "def.class" | "def.class.exported.decl" => "Class",
                     "def.var.exported.decl" | "def.var.exported.decl.var" | "def.var.toplevel" => "Variable",
                     _ => "Definition" // Fallback
                 }.to_string());
                 // Usar el nodo de esta captura para la línea, ya que representa el constructo principal
                 node_for_line = Some(cap.node); 
            }
        }

        // Si no encontramos un nodo específico para la línea (quizás la consulta solo tenía @def.name?)
        // usamos el primer nodo del match como fallback razonable.
        if node_for_line.is_none() {
             if let Some(first_capture) = mat.captures.first() {
                 node_for_line = Some(first_capture.node);
             }
        }

        // Si tenemos toda la información necesaria, la añadimos
        if let (Some(name), Some(kind), Some(node)) = (definition_name, kind_str, node_for_line) {
            if !name.is_empty() { // Asegurarnos de que el nombre no esté vacío
                definitions.push(DetectedDefinition {
                    source_file: path.to_path_buf(),
                    symbol_name: name,
                    kind: kind,
                    line_number: node.start_position().row + 1, // tree-sitter es 0-indexed
                });
            }
        }
    }
    // --- Fin de la consulta de Definiciones ---

    (connections, definitions) // Devolver ambos vectores
}


// --- Funciones Públicas Principales ---


pub fn start_analysis(path_to_scan: PathBuf) -> Receiver<AnalysisResult> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let root_path = path_to_scan;
        let walker: Vec<_> = WalkDir::new(&root_path)
            .into_iter()
            .filter_entry(|e| !is_ignored(e))
            .filter_map(|e| e.ok())
            .filter(|entry| entry.path().is_file() && !is_ignored(entry))
            .collect();

        let results: Vec<(PathBuf, Vec<DetectedConnection>, Vec<DetectedDefinition>)> = walker
            .par_iter()
            .map(|entry| {
                let path = entry.path().to_path_buf();
                let (connections, definitions) = analyze_file_content(&path);
                (path, connections, definitions)
            })
            .collect();

        let mut files = Vec::with_capacity(results.len());
        let mut connections = Vec::new();
        let mut definitions = Vec::new();
        for (path, file_connections, file_definitions) in results {
            files.push(path);
            connections.extend(file_connections);
            definitions.extend(file_definitions);
        }

        let result = Ok((root_path, files, connections, definitions));
        tx.send(result).ok();
    });

    rx
}


// pub fn generate_context_text(
//     root_path: &Path,
//     files: &[PathBuf],
//     connections: &[DetectedConnection],
//     include_content: bool,
// ) -> String {
//     let mut context = String::new();

//     context.push_str(&generate_structure_section(root_path, files));
//     context.push_str("\n"); 
//     context.push_str(&generate_connections_section(root_path, connections));

//     if include_content {
//         context.push_str("\n"); 
//         context.push_str(&generate_file_content_section(root_path, files));
//     }

//     context
// } 