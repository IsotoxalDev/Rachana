use std::path::PathBuf;

use gpui::*;

use crate::model::Document;
use crate::ui::document_view::{DocumentView, DocumentViewEvent};
use crate::ui::file_dialog::{pick_file_subprocess, DialogMode};
use crate::ui::library::{LibraryEvent, LibraryView};
use crate::ui::modal::ModalState;

/// Root view for the window: switches between the Films library screen and
/// the screenplay editor.
pub struct AppRoot {
    screen: Screen,
    dialog_open: bool,
    _subscriptions: Vec<Subscription>,
}

enum Screen {
    Library(Entity<LibraryView>),
    Document(Entity<DocumentView>),
}

impl AppRoot {
    /// `initial_document` comes from a CLI path argument or TESTING mode —
    /// when present, the editor opens directly instead of the library.
    pub fn new(
        initial_document: Option<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let library = cx.new(|cx| LibraryView::new(cx));
        let mut this = Self {
            screen: Screen::Library(library),
            dialog_open: false,
            _subscriptions: Vec::new(),
        };

        if let Some(doc) = initial_document {
            let modal = cx.new(|_| new_modal());
            let document = cx.new(|_| doc);
            this.open_document(document, modal, window, cx);
        } else {
            this.rebind_library(window, cx);
        }

        this
    }

    fn rebind_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let library = cx.new(|cx| LibraryView::new(cx));
        let sub = cx.subscribe_in(&library, window, Self::on_library_event);
        self.screen = Screen::Library(library);
        self._subscriptions = vec![sub];
        cx.notify();
    }

    fn open_document(
        &mut self,
        document: Entity<Document>,
        modal: Entity<ModalState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let doc_view = cx.new(|cx| DocumentView::new(document, modal, window, cx));
        let sub = cx.subscribe_in(&doc_view, window, Self::on_document_event);
        self.screen = Screen::Document(doc_view);
        self._subscriptions = vec![sub];
        cx.notify();
    }

    fn on_library_event(
        &mut self,
        _library: &Entity<LibraryView>,
        event: &LibraryEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            LibraryEvent::OpenScript(path) => self.open_script_path(path.clone(), window, cx),
            LibraryEvent::CreateScript { title, author } => {
                let modal = cx.new(|_| new_modal());
                let mut doc = Document::new();
                doc.apply_title_page_fields(title, "", author, "", "", "", "");
                let document = cx.new(|_| doc);
                self.open_document(document, modal, window, cx);
            }
            LibraryEvent::OpenFileRequested => self.open_file_dialog(window, cx),
        }
    }

    fn on_document_event(
        &mut self,
        _doc_view: &Entity<DocumentView>,
        event: &DocumentViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DocumentViewEvent::BackToLibrary => self.rebind_library(window, cx),
        }
    }

    fn open_script_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match crate::io::load_fountain(&path) {
            Ok(doc) => {
                crate::settings::touch_recent(&path, &doc.title, &doc.author, doc.scene_count());
                let modal = cx.new(|_| new_modal());
                let document = cx.new(|_| doc);
                self.open_document(document, modal, window, cx);
            }
            Err(e) => eprintln!("error opening {}: {e}", path.display()),
        }
    }

    fn open_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;

        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Fountain Script".into()),
        });
        cx.spawn_in(window, async move |weak_this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(mut paths))) => paths.pop(),
                Ok(Ok(None)) => {
                    weak_this.update(cx, |this, _| this.dialog_open = false).ok();
                    return;
                }
                Ok(Err(_)) | Err(_) => {
                    pick_file_subprocess(DialogMode::Open, "fountain", "Fountain Files (*.fountain)").await
                }
            };
            weak_this.update_in(cx, |this, window, cx| {
                this.dialog_open = false;
                if let Some(p) = path {
                    this.open_script_path(p, window, cx);
                }
            }).ok();
        }).detach();
    }
}

fn new_modal() -> ModalState {
    let mut m = ModalState::new();
    if crate::settings::load().vim_enabled {
        m.toggle_vim();
    }
    m
}

impl Render for AppRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        match &self.screen {
            Screen::Library(library) => div().size_full().child(library.clone()),
            Screen::Document(doc_view) => div().size_full().child(doc_view.clone()),
        }
    }
}
