#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Ocultar consola en Windows release

mod analysis;
mod reporting;

use std::path::{ PathBuf};
use std::sync::mpsc::{ Receiver};
use std::time::{Duration, Instant};

use analysis::{AnalysisResult, DetectedDefinition, ResolvedConnection};
use arboard::Clipboard;

#[derive(Clone, Debug)]
enum ScanStatus {
    Idle,
    Scanning,
    Completed(PathBuf, Vec<PathBuf>, Vec<ResolvedConnection>, Vec<DetectedDefinition>),
    Error(String),
}

impl Default for ScanStatus {
    fn default() -> Self {
        ScanStatus::Idle
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..
        Default::default()
    };

    eframe::run_native(
        "Project Context Extractor (MVP)",
        options,
        Box::new(|_cc| Box::<MyApp>::default()),
    )
}

struct MyApp {
    scan_status: ScanStatus,
    scan_receiver: Option<Receiver<AnalysisResult>>,
    include_file_content: bool,
    copy_notification: Option<Instant>,

    // --- Generated Section Content ---
    // Now storing structured data for interactivity
    structure_section: Option<Vec<reporting::ReportItem>>,
    connections_section: Option<Vec<reporting::ReportItem>>,
    file_content_section: Option<String>, // Keep as String for now
    definitions_section: Option<Vec<reporting::ReportItem>>, // Updated to Vec<ReportItem>
    inverse_usage_section: Option<Vec<reporting::ReportItem>>, // Updated to Vec<ReportItem>

    // --- UI State ---
    show_structure: bool,
    show_connections: bool,
    show_definitions: bool,
    show_inverse_usage: bool,
    show_file_content: bool,

    // --- State for section filtering ---
    filter_structure: String,
    filter_connections: String,
    filter_definitions: String,
    filter_inverse_usage: String,
    // Note: Filtering file content directly might be too slow/complex for now

    // --- Modal State ---
    show_modal: bool,
    modal_file_path: Option<PathBuf>,
    modal_file_content: Option<String>,
    modal_copy_include_path: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            scan_status: ScanStatus::Idle,
            scan_receiver: None,
            include_file_content: false,
            copy_notification: None,
            structure_section: None,
            connections_section: None,
            file_content_section: None,
            definitions_section: None,
            inverse_usage_section: None,
            // Initialize visibility flags
            show_structure: true,
            show_connections: true,
            show_definitions: true,
            show_inverse_usage: true,
            show_file_content: true, // Default to visible if generated

            // Initialize filter strings
            filter_structure: String::new(),
            filter_connections: String::new(),
            filter_definitions: String::new(),
            filter_inverse_usage: String::new(),

            // Initialize modal state
            show_modal: false,
            modal_file_path: None,
            modal_file_content: None,
            modal_copy_include_path: false,
        }
    }
}

// --- Funciones Helper para UI ---

fn copy_to_clipboard(text_to_copy: &str, copy_notification: &mut Option<Instant>) {
    match Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(text_to_copy) {
                eprintln!("Error al copiar al portapapeles: {}", e);
                *copy_notification = None; 
            } else {
                *copy_notification = Some(Instant::now());
            }
        }
        Err(e) => {
            eprintln!("Error al inicializar el portapapeles: {}", e);
             *copy_notification = None;
        }
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut trigger_section_generation = false;
        let mut trigger_content_generation_only = false;

        if let Some(rx) = &self.scan_receiver {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok((root_path, files, connections, definitions)) => {
                        self.scan_status = ScanStatus::Completed(root_path, files, connections, definitions);
                        trigger_section_generation = true;
                    }
                    Err(err_msg) => {
                        self.scan_status = ScanStatus::Error(err_msg);
                        self.clear_generated_sections();
                    }
                }
                self.scan_receiver = None;
            } else {
                 ctx.request_repaint();
            }
        }

        // --- Panel Superior ---
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                 
                let analysis_button_enabled = !matches!(self.scan_status, ScanStatus::Scanning);
                let analysis_button_text = match self.scan_status { ScanStatus::Scanning => "Analizando...", _ => "Analizar Proyecto" };
                if ui.add_enabled(analysis_button_enabled, egui::Button::new(analysis_button_text)).clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.scan_status = ScanStatus::Scanning;
                        self.clear_generated_sections();
                        self.scan_receiver = Some(analysis::start_analysis(path));
                    }
                }
                ui.separator();

                
                let is_completed = matches!(self.scan_status, ScanStatus::Completed(_, _, _, _));
                let checkbox_changed = ui.add_enabled(is_completed, egui::Checkbox::new(&mut self.include_file_content, "Incluir contenido")).changed();
                if checkbox_changed && is_completed {
                    trigger_content_generation_only = true;
                }
                ui.separator();
                
                
                let copy_enabled = is_completed;
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Estructura")).clicked() {
                    if let Some(items) = &self.structure_section {
                        // Convert ReportItems to String before copying
                        let text_to_copy = Self::report_items_to_string(items);
                        copy_to_clipboard(&text_to_copy, &mut self.copy_notification);
                    }
                }
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Conexiones")).clicked() {
                    if let Some(items) = &self.connections_section {
                        // Convert ReportItems to String before copying
                        let text_to_copy = Self::report_items_to_string(items);
                        copy_to_clipboard(&text_to_copy, &mut self.copy_notification);
                    }
                }
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Definiciones")).clicked() {
                    if let Some(items) = &self.definitions_section {
                        // Convert ReportItems to String before copying
                        let text_to_copy = Self::report_items_to_string(items);
                        copy_to_clipboard(&text_to_copy, &mut self.copy_notification);
                    }
                }
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Usos")).clicked() {
                    if let Some(items) = &self.inverse_usage_section {
                        // Convert ReportItems to String before copying
                        let text_to_copy = Self::report_items_to_string(items);
                        copy_to_clipboard(&text_to_copy, &mut self.copy_notification);
                    }
                }
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Todo")).clicked() {
                     let full_context = self.rebuild_full_context();
                    copy_to_clipboard(&full_context, &mut self.copy_notification);
                }

                
                if let Some(copy_time) = self.copy_notification {
                    if copy_time.elapsed() < Duration::from_secs(2) {
                         ui.label(egui::RichText::new("¡Copiado!").color(egui::Color32::GREEN));
                    } else {
                        self.copy_notification = None;
                    }
                }
            });
        });

        // --- Left Sidebar for Visibility Control ---
        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(150.0)
            .show(ctx, |ui| {
                ui.heading("Mostrar Secciones");
                ui.separator();
                ui.checkbox(&mut self.show_structure, "Estructura");
                ui.checkbox(&mut self.show_connections, "Conexiones");
                ui.checkbox(&mut self.show_definitions, "Definiciones");
                ui.checkbox(&mut self.show_inverse_usage, "Usos Inversos");
                ui.add_enabled(self.include_file_content, egui::Checkbox::new(&mut self.show_file_content, "Contenido Archivos"));
                ui.separator();

                // --- Filter Inputs ---
                ui.heading("Filtrar");
                ui.label("Estructura:");
                ui.text_edit_singleline(&mut self.filter_structure);
                ui.label("Conexiones:");
                ui.text_edit_singleline(&mut self.filter_connections);
                ui.label("Definiciones:");
                ui.text_edit_singleline(&mut self.filter_definitions);
                 ui.label("Usos Inversos:");
                ui.text_edit_singleline(&mut self.filter_inverse_usage);
                // ---------------------

                // Ensure visibility is off if generation is off
                if !self.include_file_content {
                    self.show_file_content = false;
                }

                // TODO: Add filtering controls here in the future?
            });

        
        // --- Section Generation Logic (Applying Filters) ---
        if trigger_section_generation || 
           // Regenerate sections if filters change and we have data
           (matches!(self.scan_status, ScanStatus::Completed(_,_,_,_)) && 
            (self.filter_structure.len() > 0 || self.filter_connections.len() > 0 || 
             self.filter_definitions.len() > 0 || self.filter_inverse_usage.len() > 0))
         {
             if let ScanStatus::Completed(root_path, files, connections, definitions) = &self.scan_status {
                // Apply filters BEFORE generating sections
                
                // Filter Files for Structure Section
                let filtered_files: Vec<PathBuf> = files.iter()
                    .filter(|path| {
                        if self.filter_structure.is_empty() { return true; }
                        path.strip_prefix(root_path).unwrap_or(path)
                           .to_string_lossy().to_lowercase()
                           .contains(&self.filter_structure.to_lowercase())
                    })
                    .cloned()
                    .collect();
                self.structure_section = Some(reporting::generate_structure_section(root_path, &filtered_files));

                // Filter Connections for Connections Section
                let filtered_connections: Vec<ResolvedConnection> = connections.iter()
                    .filter(|conn| {
                        if self.filter_connections.is_empty() { return true; }
                        let filter_lower = self.filter_connections.to_lowercase();
                        let source_match = conn.source_file.strip_prefix(root_path).unwrap_or(&conn.source_file)
                                           .to_string_lossy().to_lowercase().contains(&filter_lower);
                        let import_match = conn.imported_string.to_lowercase().contains(&filter_lower);
                        let target_match = conn.resolved_target.as_ref().map_or(false, |target| {
                            target.strip_prefix(root_path).unwrap_or(target)
                                  .to_string_lossy().to_lowercase().contains(&filter_lower)
                        });
                        source_match || import_match || target_match
                    })
                    .cloned()
                    .collect();
                 self.connections_section = Some(reporting::generate_connections_section(root_path, &filtered_connections));

                 // Filter Definitions for Definitions Section
                 let filtered_definitions: Vec<DetectedDefinition> = definitions.iter()
                     .filter(|def| {
                         if self.filter_definitions.is_empty() { return true; }
                         let filter_lower = self.filter_definitions.to_lowercase();
                         let source_match = def.source_file.strip_prefix(root_path).unwrap_or(&def.source_file)
                                            .to_string_lossy().to_lowercase().contains(&filter_lower);
                         let symbol_match = def.symbol_name.to_lowercase().contains(&filter_lower);
                         let kind_match = def.kind.to_lowercase().contains(&filter_lower);
                         source_match || symbol_match || kind_match
                     })
                     .cloned()
                     .collect();
                 self.definitions_section = Some(reporting::generate_definitions_section(root_path, &filtered_definitions));

                 // Filter Connections for Inverse Usage Section
                 let filtered_connections_for_inverse: Vec<ResolvedConnection> = connections.iter()
                     .filter(|conn| {
                         if self.filter_inverse_usage.is_empty() { return true; }
                         let filter_lower = self.filter_inverse_usage.to_lowercase();
                         let source_match = conn.source_file.strip_prefix(root_path).unwrap_or(&conn.source_file)
                                            .to_string_lossy().to_lowercase().contains(&filter_lower);
                         let target_match = conn.resolved_target.as_ref().map_or(false, |target| {
                            target.strip_prefix(root_path).unwrap_or(target)
                                  .to_string_lossy().to_lowercase().contains(&filter_lower)
                        });
                         source_match || target_match
                     })
                     .cloned()
                     .collect();
                 self.inverse_usage_section = Some(reporting::generate_inverse_usage_section(root_path, &filtered_connections_for_inverse));
                 
                 // File content generation remains unchanged (not filtered currently)
                 if self.include_file_content {
                     self.file_content_section = Some(reporting::generate_file_content_section(root_path, files));
                 } else {
                     self.file_content_section = None;
                 }
            }
        } else if trigger_content_generation_only {
            if let ScanStatus::Completed(root_path, files, _, _) = &self.scan_status {
                 if self.include_file_content {
                     self.file_content_section = Some(reporting::generate_file_content_section(root_path, files));
                 } else {
                     self.file_content_section = None;
                 }
            }
        }

        
        egui::CentralPanel::default().show(ctx, |ui| {
           ui.heading("Project Context Extractor"); ui.separator();
             match &self.scan_status {
                ScanStatus::Idle => { ui.label("Selecciona una carpeta de proyecto para analizar."); }
                ScanStatus::Scanning => { ui.horizontal(|ui| { ui.spinner(); ui.label("Analizando archivos..."); }); }
                ScanStatus::Completed(root_path, _, _, _) => {
                    ui.label(format!("Carpeta analizada: {}", root_path.display()));
                    ui.separator();
                    let mut clicked_path_in_scroll: Option<PathBuf> = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        // Borrow self immutably within the scroll area
                        let app_state = &*self; // Use immutable borrow inside closure
                        
                        if app_state.show_structure {
                            if let Some(structure) = &app_state.structure_section {
                                // Display section and capture potential click
                                if let Some(path) = Self::display_section(ui, "structure_section", structure) {
                                    clicked_path_in_scroll = Some(path);
                                }
                                ui.separator();
                            }
                        }
                        if app_state.show_connections {
                            if let Some(connections) = &app_state.connections_section {
                                // Pass the &[ReportItem] slice directly
                                if let Some(path) = Self::display_section(ui, "connections_section", connections) {
                                     clicked_path_in_scroll = Some(path);
                                }
                                ui.separator();
                            }
                        }
                        if app_state.show_definitions {
                            if let Some(definitions) = &app_state.definitions_section {
                                // Actualizado: ahora usa ReportItem
                                if let Some(path) = Self::display_section(ui, "definitions_section", definitions) {
                                    clicked_path_in_scroll = Some(path);
                                }
                                ui.separator();
                            }
                        }
                        if app_state.show_inverse_usage {
                            if let Some(inverse_usage) = &app_state.inverse_usage_section {
                                // Actualizado: ahora usa ReportItem
                                if let Some(path) = Self::display_section(ui, "inverse_usage_section", inverse_usage) {
                                    clicked_path_in_scroll = Some(path);
                                }
                                ui.separator();
                            }
                        }
                        // File content display remains the same for now
                        if app_state.include_file_content && app_state.show_file_content {
                            if let Some(content) = &app_state.file_content_section {
                                ui.strong("Contenido de Archivos"); // Temporary heading
                                ui.add_space(2.0);
                                let mut text = content.clone();
                                ui.add(egui::TextEdit::multiline(&mut text).code_editor().desired_width(f32::INFINITY));
                            }
                        }
                    }); // End of ScrollArea

                    // -- Handle click AFTER ScrollArea --
                    if let Some(path) = clicked_path_in_scroll {
                        self.show_modal = true;
                        self.modal_file_path = Some(path.clone());
                        match std::fs::read_to_string(&path) {
                            Ok(content) => self.modal_file_content = Some(content),
                            Err(e) => self.modal_file_content = Some(format!("[Error al leer el archivo: {}]", e)),
                        }
                    }
                }
                ScanStatus::Error(msg) => { ui.colored_label(egui::Color32::RED, format!("Error: {}", msg)); }
            }
        });

        // --- Modal Window Logic ---
        if self.show_modal {
            let mut is_open = true; // Control variable for the window
            let file_name = self.modal_file_path.as_ref()
                              .and_then(|p| p.file_name())
                              .and_then(|n| n.to_str())
                              .unwrap_or("Archivo");
            
            egui::Window::new(format!("Contenido: {}", file_name))
                .open(&mut is_open)
                .default_width(600.0)
                .default_height(400.0)
                .resizable(true)
                .scroll2([true, true]) // Enable scrolling
                .show(ctx, |ui| {
                    // Add a copy button and checkbox at the top
                    ui.horizontal(|ui|{
                        if ui.button("Copiar Contenido").clicked() {
                            if let Some(content) = &self.modal_file_content {
                                let mut text_to_copy = content.clone();
                                // Prepend path if checkbox is checked and path exists
                                if self.modal_copy_include_path {
                                    if let Some(path) = &self.modal_file_path {
                                        let path_str = path.display().to_string();
                                        // Use a common comment style (adjust if needed for specific languages later)
                                        text_to_copy = format!("// File: {}\n\n{}", path_str, content);
                                    }
                                }
                                copy_to_clipboard(&text_to_copy, &mut self.copy_notification);
                            }
                        }
                        // Checkbox to include path
                        ui.checkbox(&mut self.modal_copy_include_path, "Incluir path");
                        
                        // Display copy notification within the modal as well
                         if let Some(copy_time) = self.copy_notification {
                            if copy_time.elapsed() < Duration::from_secs(2) {
                                ui.label(egui::RichText::new(" ¡Copiado!").color(egui::Color32::GREEN));
                            } // Resetting happens in the main UI update
                        }
                    });
                    ui.separator();

                    if let Some(content) = &self.modal_file_content {
                         // Use a text edit for selection and copying, but make it read-only
                         let mut content_display = content.clone();
                         ui.add_sized(ui.available_size(), 
                            egui::TextEdit::multiline(&mut content_display)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .lock_focus(true) // Prevent accidental edits
                         );
                    } else {
                        ui.label("No se pudo cargar el contenido.");
                    }
            });

            // If the window was closed (by clicking 'x'), update the state
            if !is_open {
                self.show_modal = false;
                self.modal_file_path = None;
                self.modal_file_content = None;
            }
        }
    }
}

impl MyApp {
    // --- NEW Helper function ---
    fn report_items_to_string(items: &[reporting::ReportItem]) -> String {
        let mut result = String::new();
        for item in items {
            match item {
                reporting::ReportItem::PlainText(text) => result.push_str(text),
                // For FilePath, just use the display string for copying/full context
                reporting::ReportItem::FilePath { display, .. } => result.push_str(display),
            }
            result.push('\n'); // Add newline between items for readability
        }
        result.trim_end().to_string() // Remove trailing newline if any
    }

    fn clear_generated_sections(&mut self) {
        self.structure_section = None;
        self.connections_section = None;
        self.file_content_section = None;
        self.definitions_section = None;
        self.inverse_usage_section = None;
    }

    fn rebuild_full_context(&self) -> String {
        let mut full_context = String::new();
        if let Some(items) = &self.structure_section {
             // Convert ReportItems to String for full context
            let structure_text = Self::report_items_to_string(items);
            full_context.push_str(&structure_text);
            full_context.push_str("\n\n");
        }
        if let Some(items) = &self.connections_section {
            let connections_text = Self::report_items_to_string(items);
            full_context.push_str(&connections_text);
             full_context.push_str("\n\n");
        }
        if let Some(items) = &self.definitions_section {
            let definitions_text = Self::report_items_to_string(items);
            full_context.push_str(&definitions_text);
            full_context.push_str("\n\n");
        }
        if let Some(items) = &self.inverse_usage_section {
            let inverse_usage_text = Self::report_items_to_string(items);
            full_context.push_str(&inverse_usage_text);
            full_context.push_str("\n\n");
        }
        if self.include_file_content {
            if let Some(fc) = &self.file_content_section {
                 full_context.push_str(fc);
            }
        }
        full_context.trim_end().to_string()
    }

    // UPDATED: Returns Option<PathBuf> on click instead of modifying state directly
    fn display_section(ui: &mut egui::Ui, id_source: &str, items: &[reporting::ReportItem]) -> Option<PathBuf> {
        let mut clicked_path: Option<PathBuf> = None;

        // Add a heading before each section
        let heading = match id_source {
            "structure_section" => "Estructura del Proyecto",
            "connections_section" => "Conexiones Detectadas", // TODO: Update when these use ReportItem
            "definitions_section" => "Definiciones y Exportaciones", // TODO: Update when these use ReportItem
            "inverse_usage_section" => "Usos Inversos", // TODO: Update when these use ReportItem
            "content_section" => "Contenido de Archivos",
            _ => "Sección", // Fallback heading
        };
        ui.strong(heading);
        ui.add_space(2.0);

        // Render items, making FilePaths clickable
        // Using a code block style for consistent spacing
        egui::Frame::none().show(ui, |ui| { // Use a frame for potential background/styling
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
            ui.vertical(|ui|{
                for item in items {
                    match item {
                        reporting::ReportItem::PlainText(text) => {
                            ui.label(text);
                        }
                        reporting::ReportItem::FilePath { display, path } => {
                            // Use a button that looks like a link for click detection
                             if ui.link(display).clicked() {
                                // Signal that this path was clicked
                                clicked_path = Some(path.clone());
                            }
                        }
                    }
                }
            });
        });

        clicked_path // Return the path if a link was clicked
    }
}
