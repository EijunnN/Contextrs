use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use walkdir::{DirEntry, WalkDir};
use std::collections::{HashSet};
use rayon::prelude::*;
use tree_sitter::{Parser, Language, Query, QueryCursor, Node};
use path_clean::PathClean;



const IGNORED_DIRS: &[&str] = &["node_modules", ".git", ".next", ".cursor", "target"];
const IGNORED_FILES: &[&str] = &["pnpm-lock.yaml", "yarn.lock", "package-lock.json"];



#[derive(Clone, Debug)]
pub struct DetectedConnection {
    pub source_file: PathBuf,
    pub imported_string: String,

}

#[derive(Clone, Debug)]
pub struct ResolvedConnection {
    pub source_file: PathBuf,
    pub imported_string: String,
    pub resolved_target: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct DetectedDefinition {
    pub source_file: PathBuf,
    pub symbol_name: String,
    pub kind: String, // e.g., "Function", "Class", "Const", "Let", "Var", "Export"
    pub line_number: usize, // Line number where the definition starts
}


pub type AnalysisResult = Result<(PathBuf, Vec<PathBuf>, Vec<ResolvedConnection>, Vec<DetectedDefinition>), String>;

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

    // --- Consulta de Definiciones (Adaptada por lenguaje) ---
    let definition_query_str = match path.extension().and_then(|ext| ext.to_str()) {
        // JavaScript (js, jsx, mjs, cjs) usa 'identifier' para clases
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => r#"
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
    
              ; Clases (JS usa identifier)
              (class_declaration name: (identifier) @def.name) @def.class 
              (export_statement declaration: (class_declaration name: (identifier) @def.name)) @def.class.exported.decl
    
              ; Variables/Constantes
              (export_statement declaration: (lexical_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl
              (export_statement (variable_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl.var
            ]
        "#,
        // TypeScript (ts, tsx) usa 'type_identifier' para clases
        Some("ts") | Some("tsx") => r#"
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
    
              ; Clases (TS/TSX usa type_identifier)
              (class_declaration name: (type_identifier) @def.name) @def.class 
              (export_statement declaration: (class_declaration name: (type_identifier) @def.name)) @def.class.exported.decl
    
              ; Variables/Constantes
              (export_statement declaration: (lexical_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl
              (export_statement (variable_declaration (variable_declarator name: (identifier) @def.name))) @def.var.exported.decl.var
            ]
        "#,
        // Fallback: Si no es un lenguaje soportado, no intentar consulta de definiciones
        _ => {
             // Ya hemos devuelto (connections, definitions) vacíos antes si el lenguaje no es soportado,
            // pero por seguridad, retornamos de nuevo aquí si llegamos inesperadamente.
            return (connections, definitions);
        }
    };

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


// NUEVA: Función auxiliar para resolver rutas de importación
fn resolve_import_path(
    source_file: &Path,
    import_str: &str,
    project_files: &HashSet<PathBuf> // Conjunto de todos los archivos válidos del proyecto
) -> Option<PathBuf> {
    // Ignorar paquetes (sin ./) y URLs/absolutos por ahora
    if !import_str.starts_with('.') || import_str.contains(':') {
        return None;
    }

    let source_dir = source_file.parent()?;

    // Construir ruta base y limpiarla/normalizarla
    let base_path = source_dir.join(import_str);
    let cleaned_base_path = base_path.clean(); // Usa path_clean

    // Extensiones a probar
    let extensions = ["", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"];
    // Archivos índice a probar si es un directorio
    let index_files = ["index.js", "index.jsx", "index.ts", "index.tsx", "index.mjs", "index.cjs"];

    // 1. Probar como archivo con/sin extensión
    for ext in extensions {
        let mut potential_path = cleaned_base_path.clone();
        // set_extension requiere la extensión sin el punto inicial, pero sí para la comparación
        // Manejar el caso sin extensión explícitamente
        if ext.is_empty() {
             // Ya es cleaned_base_path, no hacer nada
        } else {
            // Construir el nombre de archivo con extensión
             let current_filename = potential_path.file_name().unwrap_or_default();
             let mut new_filename = current_filename.to_os_string();
            // Evitar doble extensión si ya la tiene
            if potential_path.extension().is_none() || potential_path.extension().unwrap_or_default() != ext.trim_start_matches('.') {
                 new_filename.push(ext);
                 potential_path.set_file_name(new_filename);
            }
        }

        // Normalizar DE NUEVO después de añadir/modificar extensión
        let final_path = potential_path.clean();

        if project_files.contains(&final_path) {
            return Some(final_path);
        }

        // Caso especial: si el import no tiene extensión, probar añadiéndola
        if import_str.ends_with('/') || Path::new(import_str).extension().is_none() {
            if !ext.is_empty() {
                 let mut path_with_ext = cleaned_base_path.clone();
                path_with_ext.set_extension(ext.trim_start_matches('.'));
                let final_path_with_ext = path_with_ext.clean();
                 if project_files.contains(&final_path_with_ext) {
                    return Some(final_path_with_ext);
                }
            }
        }

    }

    // 2. Probar como directorio buscando archivo index
    // (No necesitamos verificar is_dir explícitamente, path_clean maneja la base)
    for index_file in index_files {
        let potential_path = cleaned_base_path.join(index_file).clean();
        if project_files.contains(&potential_path) {
            return Some(potential_path);
        }
    }

    None // No se encontró resolución local
}


// --- Funciones Públicas Principales ---


pub fn start_analysis(path_to_scan: PathBuf) -> Receiver<AnalysisResult> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let root_path = path_to_scan;
        let walker_entries: Vec<_> = WalkDir::new(&root_path)
            .into_iter()
            .filter_entry(|e| !is_ignored(e))
            .filter_map(|e| e.ok())
            .filter(|entry| entry.path().is_file() && !is_ignored(entry))
            .collect();

        // Crear HashSet de todos los archivos encontrados para búsqueda eficiente
        let project_files_set: HashSet<PathBuf> = walker_entries
            .par_iter()
            .map(|entry| entry.path().to_path_buf().clean()) // Limpiar/normalizar aquí también
            .collect();

        // Paso 1: Análisis inicial para obtener conexiones crudas y definiciones
        let initial_results: Vec<(PathBuf, Vec<DetectedConnection>, Vec<DetectedDefinition>)> = walker_entries
            .par_iter()
            .map(|entry| {
                let path = entry.path().to_path_buf();
                let (connections, definitions) = analyze_file_content(&path);
                (path, connections, definitions)
            })
            .collect();

        let mut files = Vec::with_capacity(initial_results.len());
        let mut raw_connections = Vec::new();
        let mut definitions = Vec::new();
        for (path, file_connections, file_definitions) in initial_results {
            files.push(path.clean()); // Almacenar rutas limpias
            raw_connections.extend(file_connections);
            definitions.extend(file_definitions);
        }

        // Paso 2: Resolver las conexiones
        let resolved_connections: Vec<ResolvedConnection> = raw_connections
            .par_iter() // Paralelizar resolución si es posible/seguro
            .map(|conn| {
                let resolved = resolve_import_path(&conn.source_file, &conn.imported_string, &project_files_set);
                ResolvedConnection {
                    source_file: conn.source_file.clone().clean(), // Guardar ruta limpia
                    imported_string: conn.imported_string.clone(),
                    resolved_target: resolved, // Puede ser None
                }
            })
            .collect();

        // Ordenar archivos para consistencia
        files.sort();
        // Podríamos ordenar definiciones y conexiones si es necesario

        // Enviar el resultado con conexiones resueltas
        let result = Ok((root_path, files, resolved_connections, definitions));
        tx.send(result).ok(); // Ignorar error si el receptor ya no existe
    });

    rx
}

