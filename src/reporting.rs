use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::analysis::{DetectedConnection, DetectedDefinition}; // Añadir DetectedDefinition

// --- Funciones Movidas desde analysis.rs ---

// Helper interno para generar árbol de estructura (podría permanecer aquí o moverse si se reutiliza)
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

// --- Nueva Función para Generar Sección de Definiciones ---
pub fn generate_definitions_section(root_path: &Path, definitions: &[DetectedDefinition]) -> String {
    let mut section = String::new();
    section.push_str("## Detected Definitions & Exports\n\n");

    if definitions.is_empty() {
        section.push_str("_No definitions or exports detected._\n");
        return section;
    }

    // 1. Agrupar definiciones por archivo fuente
    let mut grouped_definitions: HashMap<PathBuf, Vec<&DetectedDefinition>> = HashMap::new();
    for def in definitions {
        grouped_definitions.entry(def.source_file.clone()).or_default().push(def);
    }

    // 2. Obtener archivos fuente ordenados
    let mut sorted_files: Vec<PathBuf> = grouped_definitions.keys().cloned().collect();
    sorted_files.sort();

    // 3. Construir la cadena de la sección
    for file_path in sorted_files {
        if let Some(defs_in_file) = grouped_definitions.get_mut(&file_path) {
            // Ordenar definiciones dentro del archivo por número de línea
            defs_in_file.sort_by_key(|d| d.line_number);

            let display_path = file_path
                .strip_prefix(root_path)
                .unwrap_or(&file_path)
                .display();

            section.push_str(&format!("### `{}`\n", display_path));
            section.push_str("```\n");

            // Calcular padding para el número de línea
            let max_line_num = defs_in_file.last().map_or(0, |d| d.line_number);
            let line_width = if max_line_num == 0 { 1 } else { max_line_num.to_string().len() };

            // Calcular padding para el tipo (Kind)
            let max_kind_len = defs_in_file.iter().map(|d| d.kind.len()).max().unwrap_or(0);

            for def in defs_in_file {
                // Restaurar formato original
                section.push_str(&format!(
                    "L{:<line_width$} {:<kind_width$} {}\n", 
                    def.line_number, 
                    def.kind, 
                    def.symbol_name, 
                    line_width = line_width, 
                    kind_width = max_kind_len
                ));
            }
            section.push_str("```\n\n");
        }
    }

    section
}

pub fn generate_file_content_section(root_path: &Path, files: &[PathBuf]) -> String {
     let mut section = String::new();
    section.push_str("## File Contents\n\n");
    let mut sorted_files = files.to_vec();
    sorted_files.sort();

    for file_path in sorted_files {
        let relative_path_display = match file_path.strip_prefix(root_path) {
            Ok(relative_path) => relative_path.display().to_string(),
            Err(_) => file_path.display().to_string(), // Use full path if strip fails
        };

        section.push_str(&format!("### `{}`\n\n", relative_path_display));
        section.push_str("```");
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            section.push_str(ext);
        }
        section.push('\n');

        match fs::read_to_string(&file_path) {
            Ok(content) => {
                let lines: Vec<&str> = content.lines().collect();
                let num_lines = lines.len();
                // Calculate padding width based on the largest line number
                let width = if num_lines == 0 { 1 } else { num_lines.to_string().len() };

                for (i, line) in lines.iter().enumerate() {
                    let line_number = i + 1;
                    section.push_str(&format!("{:<width$} | {}\n", line_number, line, width = width)); // Use left alignment for line numbers
                }
                 // Handle trailing newline correctly after loop
                 if content.ends_with('\n') && !content.is_empty() {
                    // If content ends with newline AND is not empty, the loop added the last line's \n. We are good.
                 } else if content.is_empty() {
                    // Empty file, do nothing extra.
                 } else if !content.ends_with('\n') && !lines.is_empty() {
                     // Content does not end with newline, but we added one for the last line. Remove it.
                     if section.ends_with('\n') { section.pop(); }
                 }
            }
            Err(e) => section.push_str(&format!("[Error reading file: {}]", e)),
        }

        section.push_str("\n```\n\n"); // Ensure newline before closing backticks
    }
    section
} 