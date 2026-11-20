//! What an app cannot write with rune-pele's types, so that a new field or variant breaks no app:
//! a struct literal, which a new field would break, and a match with no wildcard, which a new
//! variant would. Each block fails to compile with the error it names.
//!
//! ```compile_fail,E0639
//! let stats = rune_pele::ViewStats { paints: 1, ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let times = rune_pele::ViewTimes { ..Default::default() };
//! ```
//!
//! ```compile_fail,E0639
//! let census = rune_pele::lower::Census { elements: 1, ..Default::default() };
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::Engine;
//! fn said(engine: Engine) {
//!     match engine {
//!         Engine::Pele | Engine::RuneMermaid => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::RenderStatus;
//! fn said(status: RenderStatus) {
//!     match status {
//!         RenderStatus::Unrendered | RenderStatus::Pele | RenderStatus::RuneMermaid | RenderStatus::Kept => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::Fallback;
//! fn said(why: Fallback) {
//!     match why {
//!         Fallback::NotPorted | Fallback::NotLowered(_) => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::ViewError;
//! fn said(error: ViewError) {
//!     match error {
//!         ViewError::Core(_) | ViewError::NoFallback | ViewError::Option(_) | ViewError::Geometry(_) => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::lower::Unsupported;
//! fn said(why: Unsupported) {
//!     match why {
//!         Unsupported::Kind(_)
//!         | Unsupported::Style(_)
//!         | Unsupported::CrossingFill
//!         | Unsupported::Image
//!         | Unsupported::Gradient => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::KeptLook;
//! fn said(look: KeptLook) {
//!     match look {
//!         KeptLook::DimmedWithError | KeptLook::Unchanged => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::IconPaint;
//! fn said(paint: IconPaint) {
//!     match paint {
//!         IconPaint::Current | IconPaint::Fixed(_) => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::IconError;
//! fn said(error: IconError) {
//!     match error {
//!         IconError::ViewBox | IconError::StrokeWidth | IconError::PathData { .. } => {}
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use rune_pele::Event;
//! fn said(event: Event) {
//!     match event {
//!         Event::Activate(_) => {}
//!     }
//! }
//! ```
