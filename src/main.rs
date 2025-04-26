#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // Ocultar consola en Windows release

mod analysis;

use std::path::{ PathBuf};
use std::sync::mpsc::{ Receiver};
use std::time::{Duration, Instant};


use analysis::{AnalysisResult, DetectedConnection};
use arboard::Clipboard;

#[derive(Clone, Debug)]
enum ScanStatus {
    Idle,
    Scanning,
    Completed(PathBuf, Vec<PathBuf>, Vec<DetectedConnection>),
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

    structure_section: Option<String>,
    connections_section: Option<String>,
    file_content_section: Option<String>,
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
                    Ok((root_path, files, connections)) => {
                        self.scan_status = ScanStatus::Completed(root_path, files, connections);
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

                
                let is_completed = matches!(self.scan_status, ScanStatus::Completed(_, _, _));
                let checkbox_changed = ui.add_enabled(is_completed, egui::Checkbox::new(&mut self.include_file_content, "Incluir contenido")).changed();
                if checkbox_changed && is_completed {
                    trigger_content_generation_only = true;
                }
                ui.separator();
                
                
                let copy_enabled = is_completed;
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Estructura")).clicked() {
                    if let Some(text) = &self.structure_section {
                        copy_to_clipboard(text, &mut self.copy_notification);
                    }
                }
                if ui.add_enabled(copy_enabled, egui::Button::new("Copiar Conexiones")).clicked() {
                    if let Some(text) = &self.connections_section {
                        copy_to_clipboard(text, &mut self.copy_notification);
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

        
        if trigger_section_generation {
             if let ScanStatus::Completed(root_path, files, connections) = &self.scan_status {
                 self.structure_section = Some(analysis::generate_structure_section(root_path, files));
                 self.connections_section = Some(analysis::generate_connections_section(root_path, connections));
                 if self.include_file_content {
                     self.file_content_section = Some(analysis::generate_file_content_section(root_path, files));
                 } else {
                     self.file_content_section = None;
                 }
            }
        } else if trigger_content_generation_only {
            if let ScanStatus::Completed(root_path, files, _) = &self.scan_status {
                 if self.include_file_content {
                     self.file_content_section = Some(analysis::generate_file_content_section(root_path, files));
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
                ScanStatus::Completed(root_path, _, _) => {
                    ui.label(format!("Carpeta analizada: {}", root_path.display()));
                    ui.separator();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(structure) = &self.structure_section {
                            Self::display_section(ui, "structure_section", structure);
                        }
                         if let Some(connections) = &self.connections_section {
                             ui.separator();
                            Self::display_section(ui, "connections_section", connections);
                        }
                        if self.include_file_content {
                            if let Some(content) = &self.file_content_section {
                                 ui.separator();
                                Self::display_section(ui, "content_section", content);
                            }
                        }
                    });
                }
                ScanStatus::Error(msg) => { ui.colored_label(egui::Color32::RED, format!("Error: {}", msg)); }
            }
        });
    }
}

impl MyApp {
    fn clear_generated_sections(&mut self) {
        self.structure_section = None;
        self.connections_section = None;
        self.file_content_section = None;
    }

    fn rebuild_full_context(&self) -> String {
        let mut full_context = String::new();
        if let Some(s) = &self.structure_section {
            full_context.push_str(s);
            full_context.push_str("\n\n");
        }
        if let Some(c) = &self.connections_section {
            full_context.push_str(c);
             full_context.push_str("\n\n");
        }
        if self.include_file_content {
            if let Some(fc) = &self.file_content_section {
                 full_context.push_str(fc);
            }
        }
        full_context.trim_end().to_string()
    }

    fn display_section(ui: &mut egui::Ui, id_source: &str, text_content: &str) {
         let mut display_text = text_content.to_string();
         ui.add(
            egui::TextEdit::multiline(&mut display_text)
                .id_source(id_source) 
                .code_editor()
                .desired_width(f32::INFINITY)
                .lock_focus(true)
        );
    }
}
