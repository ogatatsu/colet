//! Naming a new folder, or an item renamed: the dialog that asks, and whether the folder has the name
//! already, asked off the UI thread.
use super::*;

impl Colet {
    /// F2: a rename, or for a saved search or favorite in home, its edit.
    pub fn ask_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, reasons, entries) = (self.picked(), self.reasons(cx), self.picked_entries());
        let at_home = self.action_column().is_some_and(|c| c.kind == ColumnKind::Home);
        self.take_menu();
        if let [path] = picked.as_slice() {
            if HomeKey::of(path).is_search() {
                self.open_edit(path, window, cx);
                return;
            }
            if at_home && self.stored.settings().is_favorite(path) {
                self.open_favorite_edit(path, window, cx);
                return;
            }
        }
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = reasons.rename {
            self.refuse("名前を変更", reason, cx);
            return;
        }
        let Some(entry) = entries.into_iter().next() else { return };
        // Like Explorer, the extension stays out of the selection.
        let end = if entry.directory { entry.name.len() } else { entry.name.rfind('.').filter(|&d| d > 0).unwrap_or(entry.name.len()) };
        let request = Request { kind: Kind::Rename, paths: vec![entry.path], destination: PathBuf::new(), name: entry.name };
        self.ask_name(request, end, window, cx);
    }

    pub fn ask_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.take_menu();
        if let Some(reason) = self.reasons(cx).paste {
            self.refuse("新規フォルダー", reason, cx);
            return;
        }
        let destination = self.destination().unwrap_or_default();
        let name = "新しいフォルダー".to_owned();
        let end = name.len();
        self.ask_name(Request { kind: Kind::NewFolder, paths: Vec::new(), destination, name }, end, window, cx);
    }

    /// The name dialog for `request`, starting from its name with the first `select_end` bytes selected.
    pub(super) fn ask_name(&mut self, request: Request, select_end: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(request.name.clone()));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.submit_name(window, cx),
            InputEvent::Change => {
                if let Some(Dialog::Name { tried, checking, .. }) = this.overlays.dialog_mut() {
                    *tried = false;
                    // The answer on its way is about a name no longer there.
                    *checking = None;
                }
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        cx.palette().style_input(&input, cx);
        self.overlays.listen_in_dialog(subscription);
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..select_end, cx);
        });
        self.overlays.set_dialog(Dialog::Name { request, input, tried: false, taken: None, checking: None });
        cx.notify();
    }

    /// Why the name in the dialog cannot be used, as it stands.
    pub fn name_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Name { input, tried, taken, .. }) = self.overlays.dialog() else { return None };
        let name = input.read(cx).value();
        colet_core::operations::name_problem(&name, *tried)
            .or(taken.as_deref().filter(|&taken| taken == name.as_ref()).map(|_| "同じ名前の項目があります"))
    }

    pub fn submit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { request, input, tried, checking, .. }) = self.overlays.dialog_mut() else { return };
        let name = input.read(cx).value().to_string();
        // Pressed again while the folder is asked about this very name: the answer sends it.
        if name.trim().is_empty() || checking.as_deref() == Some(name.as_str()) {
            return;
        }
        // Like Explorer, an unchanged name just closes the dialog: one that is not all text, as it was
        // shown, too, rather than renamed to what stood in for what is not.
        if request.kind == Kind::Rename && request.paths.first().is_some_and(|p| shown(folder_name(p)) == name) {
            self.close_dialog(window, cx);
            return;
        }
        // Say what the name still lacks rather than send it.
        if colet_core::operations::name_problem(&name, true).is_some() {
            *tried = true;
            cx.notify();
            return;
        }
        // Nor a name its folder already has, unless it is the item's own in another case. The folder is asked
        // off the UI thread, as a share can keep it waiting; the dialog stays up until it answers.
        if request.kind == Kind::Rename
            && let Some(path) = request.paths.first()
            && shown(folder_name(path)).to_lowercase() != name.to_lowercase()
            && let Some(parent) = parent_folder(path)
        {
            let target = child(parent, &name);
            *checking = Some(name.clone());
            let (backend, dialog) = (self.backend.clone(), input.entity_id());
            let asked = cx.background_executor().spawn(async move { backend.path_kind(&target) != PathKind::Gone });
            cx.spawn_in(window, async move |this, cx| {
                let exists = asked.await;
                this.update_in(cx, |this, window, cx| this.name_checked(dialog, name, exists, window, cx)).ok();
            })
            .detach();
            cx.notify();
            return;
        }
        self.send_name(name, window, cx);
    }

    /// The folder answered whether it has `name`: taken, the dialog says so; free, it is sent. Only while the
    /// same dialog still waits on that name: closed, or typed on since, the answer is about nothing shown.
    pub(super) fn name_checked(&mut self, dialog: EntityId, name: String, exists: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { input, taken, checking, .. }) = self.overlays.dialog_mut() else { return };
        if input.entity_id() != dialog || checking.as_deref() != Some(name.as_str()) {
            return;
        }
        *checking = None;
        if exists {
            *taken = Some(name);
            cx.notify();
            return;
        }
        self.send_name(name, window, cx);
    }

    pub(super) fn send_name(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { request, .. }) = self.overlays.dialog() else { return };
        let request = Request { name, ..request.clone() };
        self.close_dialog(window, cx);
        self.run(request, cx);
    }

    /// A new folder's name, or a rename's.
    pub(crate) fn name_dialog(
        &self,
        request: &Request,
        input: &Entity<InputState>,
        checking: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> DialogParts {
        let p = cx.palette();
        let Request { kind, paths, destination, .. } = request;
        let problem = self.name_problem(cx);
        let empty = input.read(cx).value().trim().is_empty();
        // Where the name goes: a rename may come from the results, far from its folder.
        let (place, submit) = match kind {
            Kind::NewFolder => (format!("作成先：{}", shown(destination)), "作成"),
            _ => (
                format!("場所：{}", paths.first().and_then(|p| ancestors(p).into_iter().rev().nth(1)).map(shown).unwrap_or_default()),
                "保存",
            ),
        };
        let content =
            div().flex().flex_col().gap_3().child(div().text_size(scaled(metrics::TEXT_BODY)).text_color(p.text_faint).child(place)).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "名前"))
                    .child(field(p, input, self.has_keys(input, window, cx), Self::right_press_menu(input, cx)))
                    .when_some(problem, |d, problem| {
                        d.child(div().text_size(scaled(metrics::TEXT_BODY)).text_color(p.danger).child(problem))
                    }),
            );
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            // While the folder is asked whether the name is free, the press is in and waits.
            (
                "submit-name",
                push("submit-name", if checking { "確認中…" } else { submit }, true, !empty && problem.is_none() && !checking)
                    .on_click(cx.listener(|this, _, window, cx| this.submit_name(window, cx))),
            ),
        ];
        (if matches!(kind, Kind::NewFolder) { "新規フォルダー" } else { "名前を変更" }, content, buttons)
    }
}
