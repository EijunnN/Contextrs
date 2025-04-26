# Context Lens

Context Lens is a developer tool designed to help understand complex JavaScript/TypeScript codebases by providing a high-level overview of project structure, dependencies, and key definitions locally, without overwhelming large language models (LLMs) with excessive code.

## Use Case

When working with large or unfamiliar codebases, it's often difficult to grasp the overall architecture, understand how different parts connect, or assess the impact of potential changes. Sending entire files or projects to an LLM for analysis can be inefficient, costly (due to token limits), and may expose sensitive code.

Context Lens addresses this by:

1.  **Local Analysis:** Performing static analysis locally using `tree-sitter` to parse JS/TS/JSX/TSX code.
2.  **Contextual Summarization:** Generating concise summaries of:
    *   Project file structure.
    *   Detected module connections (imports/requires).
    *   Key definitions (functions, classes, exports).
    *   Inverse usage (which files import a specific module).
3.  **Selective Information:** Allowing users to view, filter, and copy specific sections of the context, providing only the necessary information to an LLM or for personal understanding.
4.  **Interactive Exploration:** Enabling users to quickly view the content of specific files mentioned in the reports via a modal window.

The goal is to provide just enough context to effectively query an LLM ("How does function X work?", "What uses module Y?", "What's the impact of changing class Z?") or to quickly orient a developer within the codebase.

## Current Workflow

1.  **Build and Run:**
    ```bash
    cargo build
    cargo run
    ```
2.  **Select Project Folder:** Click the "Analizar Proyecto" (Analyze Project) button and choose the root directory of the JS/TS project you want to analyze.
3.  **Analysis:** The tool will scan the project files (ignoring `node_modules`, `.git`, etc.), parse supported file types, and identify structure, connections, and definitions.
4.  **View Results:** The main panel displays the generated context, divided into sections:
    *   **Estructura (Structure):** A tree view of the project files.
    *   **Conexiones (Connections):** Shows which files import or require other resolved local files.
    *   **Definiciones (Definitions):** Lists functions, classes, and exported variables found in each file.
    *   **Usos Inversos (Inverse Usage):** Shows which files import a specific *target* file.
    *   **Contenido Archivos (File Content):** (Optional) Displays the full content of analyzed files, toggleable with the "Incluir contenido" checkbox.
5.  **Control Visibility:** Use the checkboxes in the left sidebar ("Mostrar Secciones") to toggle the visibility of each section in the main view.
6.  **Filter Results:** Use the text input fields in the left sidebar ("Filtrar") to filter the items displayed within the Structure, Connections, Definitions, and Inverse Usage sections based on file paths or symbol names.
7.  **Explore File Content:** Click on any file path displayed in the "Estructura" or "Conexiones" sections. A modal window will appear showing the content of that file.
8.  **Copy Context:**
    *   Use the "Copiar <Section>" buttons to copy individual generated sections to the clipboard.
    *   Use the "Copiar Todo" button to copy the entire visible and generated context.
    *   Within the file content modal, use the "Copiar Contenido" button (optionally check "Incluir path" to prepend the file path).
9.  **(Optional) Use with LLM:** Paste the copied context into your LLM prompt along with your specific question about the codebase.

## Future Improvements

Based on the goal of facilitating efficient codebase understanding and interaction with LLMs, the following features are planned:

1.  **Call Hierarchy Analysis:** Go beyond module imports to identify function/method calls *within* files to understand execution flow.
2.  **Enhanced Cross-Navigation:** Make definitions and usages within the report sections clickable to jump between related parts of the context (e.g., click a function usage to see its definition).
3.  **Type Information:** Extract and display type information (e.g., from TypeScript interfaces or JSDoc) associated with definitions.
4.  **Comment Linking:** Associate relevant code comments (like JSDoc) with the functions/classes they describe.
5.  **Configuration:** Allow users to customize ignored directories/files and potentially configure language-specific analysis settings.
6.  **More Language Support:** Extend analysis capabilities to other languages supported by `tree-sitter`.
7.  **UI Refinements:** Improve filtering, searching, and overall user experience. 