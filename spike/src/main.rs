//! Throwaway check of what the rewrite depends on before building on it:
//! Japanese IME in the search field, Japanese text, shell icons, a long list,
//! the Explorer context menu and IFileOperation's dialogs owned by a gpui window,
//! and following the OS light/dark setting.
#![windows_subsystem = "windows"]

use std::{collections::HashMap, sync::Arc};

use colet_core::shell::{self, IconSource, MenuThread, Operation, ShowMenu};
use gpui_kit::base::input::{Input, InputEvent, InputState};
use gpui_kit::{prelude::FluentBuilder, *};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const ROW_HEIGHT: f32 = 24.;
const ROWS: usize = 100_000;
const NAMES: [&str; 6] = ["議事録", "見積書", "写真", "Readme", "設計メモ", "ﾊﾝｶｸ"];
const EXTENSIONS: [&str; 6] = [".docx", ".xlsx", ".jpg", ".txt", ".md", ".pdf"];

/// Colet's colors, by role.
#[derive(Clone, Copy)]
struct Palette {
    bg: Hsla,
    surface: Hsla,
    border: Hsla,
    text: Hsla,
    muted: Hsla,
    hover: Hsla,
    accent_fill: Hsla,
}

impl Palette {
    fn dark() -> Self {
        Self {
            bg: hsla(210. / 360., 0.14, 0.09, 1.),
            surface: hsla(210. / 360., 0.13, 0.12, 1.),
            border: hsla(210. / 360., 0.10, 0.19, 1.),
            text: hsla(210. / 360., 0.14, 0.90, 1.),
            muted: hsla(210. / 360., 0.08, 0.62, 1.),
            hover: hsla(210. / 360., 0.12, 0.17, 1.),
            accent_fill: hsla(184. / 360., 0.42, 0.28, 1.),
        }
    }
    fn light() -> Self {
        Self {
            bg: hsla(210. / 360., 0.20, 0.96, 1.),
            surface: hsla(0., 0., 1., 1.),
            border: hsla(210. / 360., 0.14, 0.86, 1.),
            text: hsla(210. / 360., 0.20, 0.13, 1.),
            muted: hsla(210. / 360., 0.09, 0.42, 1.),
            hover: hsla(210. / 360., 0.20, 0.93, 1.),
            accent_fill: hsla(184. / 360., 0.50, 0.33, 1.),
        }
    }
    fn for_window(window: &Window) -> Self {
        // Lets the spike show the light palette without touching the OS setting.
        if let Ok(theme) = std::env::var("SPIKE_THEME") {
            return if theme == "light" { Self::light() } else { Self::dark() };
        }
        match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            _ => Self::light(),
        }
    }
}

struct Spike {
    search: Entity<InputState>,
    query: SharedString,
    icons: HashMap<IconSource, Option<Arc<RenderImage>>>,
    selected: usize,
    menu: Option<MenuThread>,
    log: Vec<String>,
    palette: Palette,
    _subscriptions: Vec<Subscription>,
}

impl Spike {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("名前やEverythingの検索式（例：ext:pdf）"));
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |this: &mut Self, state, event, _, cx| {
                if let InputEvent::Change = event {
                    this.query = state.read(cx).value();
                    cx.notify();
                }
            }),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.palette = Palette::for_window(window);
                this.say(format!("外観が変わりました: {:?}", window.appearance()));
                cx.notify();
            }),
        ];
        let menu = MenuThread::spawn();
        let mut this = Self {
            search,
            query: SharedString::default(),
            icons: HashMap::new(),
            selected: 0,
            menu: menu.as_ref().ok().cloned(),
            log: Vec::new(),
            palette: Palette::for_window(window),
            _subscriptions: subscriptions,
        };
        if let Err(error) = menu {
            this.say(format!("メニュー用スレッドを作れません: {error}"));
        }
        this.say(format!("HWND: {:#x?}", owner(window)));
        this
    }

    fn say(&mut self, line: String) {
        self.log.push(line);
        let overflow = self.log.len().saturating_sub(6);
        self.log.drain(..overflow);
    }

    fn icon(&mut self, source: IconSource) -> Option<Arc<RenderImage>> {
        self.icons
            .entry(source)
            .or_insert_with_key(|source| {
                let icon = shell::icon(source)?;
                let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
                Some(Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(buffer)])))
            })
            .clone()
    }

    fn row_name(ix: usize) -> (String, IconSource) {
        if ix.is_multiple_of(7) {
            return (format!("フォルダー {ix}"), IconSource::Folder);
        }
        let ext = EXTENSIONS[ix % EXTENSIONS.len()];
        (format!("{} {ix}{ext}", NAMES[ix % NAMES.len()]), IconSource::Extension(ext.into()))
    }

    fn show_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(menu), Some(owner)) = (self.menu.clone(), owner(window)) else { return };
        let background = std::env::temp_dir().to_string_lossy().into_owned();
        let at = cursor();
        let shown = cx.background_executor().spawn(async move {
            menu.show(ShowMenu { owner, paths: Vec::new(), background, at })
        });
        cx.spawn(async move |this, cx| {
            let result = shown.await;
            this.update(cx, |this, cx| {
                this.say(format!("右クリックメニュー: {result:?}"));
                cx.notify();
            })
        })
        .detach();
    }

    fn clash(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = owner(window) else { return };
        let dir = std::env::temp_dir().join("colet-spike");
        let file = dir.join("衝突テスト.txt");
        if std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, "colet")).is_err() {
            return;
        }
        let paths = vec![file.to_string_lossy().into_owned()];
        let destination = dir.to_string_lossy().into_owned();
        // Into its own folder without renaming, so the shell asks about the clash.
        let done = cx.background_executor().spawn(async move {

            shell::operate(&Operation::Copy { destination }, &paths, false, owner, |_| ())
        });
        cx.spawn(async move |this, cx| {
            let result = done.await;
            this.update(cx, |this, cx| {
                this.say(format!("IFileOperation: {result:?}"));
                cx.notify();
            })
        })
        .detach();
    }
}

fn owner(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

fn cursor() -> (i32, i32) {
    let mut point = windows::Win32::Foundation::POINT::default();
    let _ = unsafe { windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) };
    (point.x, point.y)
}

impl Render for Spike {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let fonts = ["Segoe UI", "Yu Gothic UI", "Meiryo UI"];
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .font_family("Segoe UI")
            .text_size(px(13.))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .p_2()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex_1()
                            .h(px(28.))
                            .px_2()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.surface)
                            .child(Input::new(&self.search)),
                    )
                    .child(
                        div()
                            .id("clash")
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(p.accent_fill)
                            .text_color(white())
                            .cursor_pointer()
                            .child("コピー衝突ダイアログ")
                            .on_click(cx.listener(|this, _, window, cx| this.clash(window, cx))),
                    ),
            )
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_b_1()
                    .border_color(p.border)
                    .child(format!("入力中の値：「{}」（{}文字）", self.query, self.query.chars().count()))
                    // Outside the BMP: 𠮷 (U+20BB7) has the longer lower stroke, unlike 吉.
                    .child(div().text_size(px(40.)).child("𠮷 吉"))
                    .children(fonts.map(|font| {
                        div()
                            .font_family(font)
                            .child(format!("{font}: 日本語の表示 漢字かなカナ ﾊﾝｶｸ ①② 𠮷野家 — Aa 0123"))
                    })),
            )
            .child({
                let view = cx.entity();
                div()
                    .id("rows")
                    .flex_1()
                    .bg(p.surface)
                    .on_mouse_down(MouseButton::Right, cx.listener(|this, _, window, cx| this.show_menu(window, cx)))
                    .child(
                        uniform_list("list", ROWS, move |range, _, cx| {
                            view.update(cx, |this, cx| {
                                range
                                    .map(|ix| {
                                        let (name, source) = Self::row_name(ix);
                                        let icon = this.icon(source);
                                        let selected = ix == this.selected;
                                        div()
                                            .id(ix)
                                            .h(px(ROW_HEIGHT))
                                            .mx_1()
                                            .px_2()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .rounded(px(4.))
                                            .when(selected, |row| row.bg(p.accent_fill).text_color(white()))
                                            .when(!selected, |row| row.hover(|row| row.bg(p.hover)))
                                            .child(match icon {
                                                Some(icon) => img(icon).size(px(16.)).into_any_element(),
                                                None => div().size(px(16.)).into_any_element(),
                                            })
                                            .child(name)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.selected = ix;
                                                cx.notify();
                                            }))
                                    })
                                    .collect()
                            })
                        })
                        .size_full(),
                    )
            })
            .child(
                div()
                    .p_2()
                    .border_t_1()
                    .border_color(p.border)
                    .text_color(p.muted)
                    .text_size(px(12.))
                    .children(self.log.iter().map(|line| div().child(line.clone())))
                    .child(format!("外観: {:?}", window.appearance())),
            )
    }
}

fn main() {
    application().run(|cx| {
        init(cx);
        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        open_window(
            WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
            cx,
            |window, cx| cx.new(|cx| Spike::new(window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
