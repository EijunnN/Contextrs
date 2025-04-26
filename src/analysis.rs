use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use walkdir::{DirEntry, WalkDir};
use regex::Regex;
use lazy_static::lazy_static; 
use std::collections::{HashMap, HashSet};
use rayon::prelude::*;



const IGNORED_DIRS: &[&str] = &["node_modules", ".git", ".next", ".cursor", "target"];
const IGNORED_FILES: &[&str] = &["pnpm-lock.yaml", "yarn.lock", "package-lock.json"];



#[derive(Clone, Debug)]
pub struct DetectedConnection {
    pub source_file: PathBuf,
    pub imported_string: String,

}


pub type AnalysisResult = Result<(PathBuf, Vec<PathBuf>, Vec<DetectedConnection>), String>;

// --- Regex ---

lazy_static! {
    
    static ref IMPORT_REGEX: Regex = Regex::new(
        r#"(?m)^[ \t]*(?:use|import|require|include|from)\s+(?:[\w:{}\s*]+?from\s+)?(?:['"]([^'"]+)['"]|([\w:]+))"#
    ).unwrap();
}


// --- Funciones Auxiliares (Internas) ---


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
    if let Ok(content) = fs::read_to_string(path) {
        for cap in IMPORT_REGEX.captures_iter(&content) {
            
            if let Some(imported) = cap.get(1).or_else(|| cap.get(2)) {
                 connections.push(DetectedConnection {
                    source_file: path.to_path_buf(),
                    imported_string: imported.as_str().trim().to_string(),
                });
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
    section.push_str("## Detected Connections (Heuristic)\n\n");
    if connections.is_empty() {
        section.push_str("_No connections detected._\n");
    } else {
        let mut sorted_connections = connections.to_vec();
        sorted_connections.sort_by_key(|c| c.source_file.clone());

        for conn in sorted_connections {
             if let Ok(relative_source) = conn.source_file.strip_prefix(root_path) {
                 section.push_str(&format!(
                    "- `{}` imports `{}`\n",
                    relative_source.display(),
                    conn.imported_string
                ));
            } else {
                 section.push_str(&format!(
                    "- `{}` imports `{}`\n",
                    conn.source_file.display(), 
                    conn.imported_string
                ));
            }
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


pub fn generate_context_text(
    root_path: &Path,
    files: &[PathBuf],
    connections: &[DetectedConnection],
    include_content: bool,
) -> String {
    let mut context = String::new();

    context.push_str(&generate_structure_section(root_path, files));
    context.push_str("\n"); 
    context.push_str(&generate_connections_section(root_path, connections));

    if include_content {
        context.push_str("\n"); 
        context.push_str(&generate_file_content_section(root_path, files));
    }

    context
} 