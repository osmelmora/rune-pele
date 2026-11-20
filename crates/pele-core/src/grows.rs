//! What an app cannot write with pele-core's types, so that a new option, field or variant breaks
//! no app: a struct literal, which a new field would break, and a match with no wildcard, which a
//! new variant would. Each block fails to compile with the error it names.
//!
//! ```compile_fail,E0639
//! let options = pele_core::LayoutOptions { padding: None, ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let options = pele_core::ParseOptions { limits: Default::default(), ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let limits = pele_core::Limits { max_edges: 1, ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let policy = pele_core::ResourcePolicy { links: false, ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let typography = pele_core::Typography { ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let rendered = pele_core::Rendered { svg: String::new(), width: 0.0, height: 0.0 };
//! ```
//!
//! ```compile_fail,E0639
//! let diagnostic = pele_core::Diagnostic {
//!     code: pele_core::ErrorCode::Syntax,
//!     kind: None,
//!     mermaid_type: None,
//!     message: String::new(),
//!     location: None,
//! };
//! ```
//!
//! ```compile_fail,E0639
//! let location = pele_core::Location { line: 1, column_utf16: 1, snippet: String::new() };
//! ```
//!
//! ```compile_fail,E0639
//! let link = pele_core::LinkRef { href: String::new(), internal: false, target: None };
//! ```
//!
//! ```compile_fail,E0004
//! use pele_core::RenderError;
//! fn said(error: RenderError) {
//!     match error {
//!         RenderError::NotPorted | RenderError::Diagnostic(_) => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use pele_core::ErrorCode;
//! fn said(code: ErrorCode) {
//!     match code {
//!         ErrorCode::Unsupported | ErrorCode::Syntax | ErrorCode::Semantic | ErrorCode::Limit => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use pele_core::MonospacePart;
//! fn said(part: MonospacePart) {
//!     match part {
//!         MonospacePart::Members | MonospacePart::Attributes | MonospacePart::Files | MonospacePart::Messages => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use pele_core::Role;
//! fn said(role: Role) {
//!     match role {
//!         Role::Background
//!         | Role::Foreground
//!         | Role::Border
//!         | Role::OnLight
//!         | Role::OnDark
//!         | Role::Line
//!         | Role::Surface
//!         | Role::SurfaceAlt
//!         | Role::Muted
//!         | Role::Accent
//!         | Role::Critical => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use pele_core::DiagramKind::*;
//! fn said(kind: pele_core::DiagramKind) {
//!     match kind {
//!         Pie | Flowchart | Class | State | Er | Requirement | Usecase | Swimlane | Agentflow
//!         | Architecture | TreeView | GitGraph | Radar | Timeline | Xychart | QuadrantChart
//!         | Sequence | Gantt | Block | C4 | Railroad | Info | Cynefin | Wardley | Venn | Packet
//!         | Eventmodeling | Treemap | Mindmap | Journey | Kanban | Ishikawa | Sankey => {}
//!     }
//! }
//! ```
