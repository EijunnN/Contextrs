use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use walkdir::{DirEntry, WalkDir};
use std::collections::{HashSet, HashMap};
use rayon::prelude::*;
use tree_sitter::{Parser, Language, Query, QueryCursor};



const IGNORED_DIRS: &[&str] = &["node_modules", ".git", ".next", ".cursor", "target"];
const IGNORED_FILES: &[&str] = &["pnpm-lock.yaml", "yarn.lock", "package-lock.json"];



#[derive(Clone, Debug)]
pub struct DetectedConnection {
    pub source_file: PathBuf,
    pub imported_string: String,

}


pub type AnalysisResult = Result<(PathBuf, Vec<PathBuf>, Vec<DetectedConnection>), String>;

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


fn analyze_file_content(path: &Path) -> Vec<DetectedConnection> {
    let mut connections = Vec::new();
    let file_content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => return connections,
    };

    let language_ref = match path.extension().and_then(|ext| ext.to_str()) {
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => unsafe { &tree_sitter_javascript() },
        Some("ts") => unsafe { &tree_sitter_typescript() },
        Some("tsx") => unsafe { &tree_sitter_tsx() },
        _ => return connections,
    };

    let mut parser = Parser::new();
    if parser.set_language(language_ref).is_err() {
        eprintln!("Error setting language for file: {}", path.display());
        return connections;
    }

    let tree = match parser.parse(&file_content, None) {
        Some(tree) => tree,
        None => {
            eprintln!("Error parsing file: {}", path.display());
            return connections;
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
            return connections;
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

    connections
}


fn generate_tree_structure_string(root_path: &Path, files: &[PathBuf]) -> String {
    let mut tree = String::new();
    let mut sorted_files = files.to_vec();
    sorted_files.sort();
    let mut printed_dirs = HashSet::new();

    for file_path in sorted_files {
        if let Ok(relative_path) = file_path.strip_prefix(root_path) {
            let components: Vec<_> = relative_path.components().collect();
             // Evitar imprimir la raíz dos veces si solo hay archivos en ella
            if components.is_empty() || (components.len() == 1 && components[0].as_os_str() == relative_path.as_os_str()) {
                 if let Some(name) = relative_path.file_name().and_then(|n| n.to_str()) {
                    tree.push_str("├── ");
                    tree.push_str(name);
                    tree.push('\n');
                }
                continue; 
            }
            
            let mut current_prefix = String::new();
            for (i, component) in components.iter().enumerate() {
                let is_last_component = i == components.len() - 1;
                let component_path = root_path.join(relative_path.iter().take(i + 1).collect::<PathBuf>());

                 if let Some(name) = component.as_os_str().to_str() {
                    
                    if !is_last_component {
                         if printed_dirs.contains(&component_path) {
                            current_prefix.push_str("│   ");
                            continue;
                        } else {
                            printed_dirs.insert(component_path);
                            tree.push_str(&current_prefix);
                            tree.push_str("├── "); 
                            tree.push_str(name);
                            tree.push_str("/\n");
                            current_prefix.push_str("│   ");
                        }
                    } else { 
                        tree.push_str(&current_prefix);
                        tree.push_str("└── "); 
                        tree.push_str(name);
                        tree.push('\n');
                    }
                 } else {
                    tree.push_str(&current_prefix);
                    tree.push_str("└── [Nombre no UTF-8]\n"); 
                    break; 
                 }
            }
        }
    }
    tree
}

// --- Generadores de Secciones (Públicos) ---
pub fn generate_structure_section(root_path: &Path, files: &[PathBuf]) -> String {
    let mut section = String::new();
    section.push_str("## Project Structure\n\n");
    section.push_str("```\n");
    section.push_str(root_path.file_name().unwrap_or_default().to_str().unwrap_or("ROOT"));
    section.push('\n');
    section.push_str(&generate_tree_structure_string(root_path, files));
    section.push_str("```\n");
    section
}


pub fn generate_connections_section(root_path: &Path, connections: &[DetectedConnection]) -> String {
    let mut section = String::new();
    section.push_str("## Detected Connections (Tree)\n\n");

    if connections.is_empty() {
        section.push_str("_No connections detected._\n");
        return section;
    }

    // 1. Group connections by source file
    let mut grouped_connections: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for conn in connections {
        grouped_connections
            .entry(conn.source_file.clone())
            .or_default()
            .push(conn.imported_string.clone());
    }

    // 2. Get sorted source files
    let mut sorted_files: Vec<PathBuf> = grouped_connections.keys().cloned().collect();
    sorted_files.sort();

    // 3. Build the tree string
    section.push_str("```\n");
    let num_files = sorted_files.len();
    for (i, file_path) in sorted_files.iter().enumerate() {
        let is_last_file = i == num_files - 1;
        let file_prefix = if is_last_file { "└── " } else { "├── " };

        // Display relative path if possible
        let display_path = file_path
            .strip_prefix(root_path)
            .unwrap_or(file_path)
            .display();

        section.push_str(&format!("{}{}\n", file_prefix, display_path));

        // Get and sort imports for this file
        if let Some(imports) = grouped_connections.get_mut(file_path) {
            imports.sort();
            let num_imports = imports.len();
            let base_indent = if is_last_file { "    " } else { "│   " };

            for (j, import_str) in imports.iter().enumerate() {
                let is_last_import = j == num_imports - 1;
                let import_prefix = if is_last_import { "└── " } else { "├── " };
                section.push_str(&format!("{}{}{}\n", base_indent, import_prefix, import_str));
            }
        }
    }
    section.push_str("```\n");

    section
}


pub fn generate_file_content_section(root_path: &Path, files: &[PathBuf]) -> String {
     let mut section = String::new();
    section.push_str("## File Contents\n\n");
    let mut sorted_files = files.to_vec();
    sorted_files.sort();

    for file_path in sorted_files {
            if let Ok(relative_path) = file_path.strip_prefix(root_path) {
            section.push_str(&format!("### `{}`\n\n", relative_path.display()));
            section.push_str("```");
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                section.push_str(ext);
            }
            section.push('\n');
            match fs::read_to_string(&file_path) {
                Ok(content) => section.push_str(&content),
                Err(e) => section.push_str(&format!("[Error reading file: {}]", e)),
            }
            section.push_str("\n```\n\n");
        } else {
                section.push_str(&format!("### `{}`\n\n", file_path.display()));
                section.push_str("```\n[Could not determine relative path]\n```\n\n");
        }
    }
    section
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

        let results: Vec<(PathBuf, Vec<DetectedConnection>)> = walker
            .par_iter()
            .map(|entry| {
                let path = entry.path().to_path_buf();
                let connections = analyze_file_content(&path);
                (path, connections)
            })
            .collect();

        let mut files = Vec::with_capacity(results.len());
        let mut connections = Vec::new();
        for (path, file_connections) in results {
            files.push(path);
            connections.extend(file_connections);
        }

        let result = Ok((root_path, files, connections));
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