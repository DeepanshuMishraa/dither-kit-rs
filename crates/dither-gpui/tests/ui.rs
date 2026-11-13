//! Headless UI integration tests: real components in a Kit test window,
//! driven by native events, asserting accessibility nodes and app results.

mod common;

use dither_gpui::prelude::*;
use gpui_kit::test::{TestSupportExt as _, TestWindowExt};
use gpui_kit::{
    AppContext, Context, Render, Role, SharedString, TestAppContext, Window, div, prelude::*, px, size,
};

struct Harness {
    clicks: u32,
    deletes: u32,
    hovered: Option<usize>,
    selected: Option<usize>,
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = format!(
            "clicks={} deletes={} hover={:?} selected={:?}",
            self.clicks, self.deletes, self.hovered, self.selected
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        DitherButton::new("deploy", "Deploy").on_click(cx.listener(|this, _, _, cx| {
                            this.clicks += 1;
                            cx.notify();
                        })),
                    )
                    .child(
                        DitherButton::new("delete", "Delete")
                            .tone(Tone::Red)
                            .disabled(true)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.deletes += 1;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div().w(px(600.0)).child(
                    DitherChart::area(
                        "traffic",
                        vec![
                            Series::new("Desktop", Tone::Blue, vec![1., 4., 2., 5., 3.]),
                            Series::new("Mobile", Tone::Pink, vec![2., 1., 3., 2., 4.]),
                        ],
                    )
                    .labels(["a", "b", "c", "d", "e"])
                    .animate(false)
                    .y_axis(false)
                    .height(px(150.0))
                    .on_hover_change(cx.listener(|this, idx: &Option<usize>, _, cx| {
                        this.hovered = *idx;
                        cx.notify();
                    }))
                    .on_selection_change(cx.listener(
                        |this, idx: &Option<usize>, _, cx| {
                            this.selected = *idx;
                            cx.notify();
                        },
                    )),
                ),
            )
            .child(DitherMeter::new("upload", 0.4, Tone::Green).label("Upload"))
            .child(
                div()
                    .id("status")
                    .role(Role::Status)
                    .test_support()
                    .aria_label(SharedString::from(status.clone()))
                    .child(status),
            )
    }
}

fn status(window: &mut Window) -> String {
    window.find("status").label().unwrap_or_default().to_string()
}

#[gpui_kit::test]
fn dither_components_respond_to_native_input(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (handle, _view) = common::open_window(cx, Some(size(px(800.0), px(600.0))), |_, cx| {
        cx.new(|_| Harness {
            clicks: 0,
            deletes: 0,
            hovered: None,
            selected: None,
        })
    });

    cx.update_window(handle.into(), |_, window, cx| {
        window.render_frame(cx);

        // Buttons: Kit semantics with the label exposed to accessibility.
        assert_eq!(window.find("deploy").label(), Some("Deploy"));
        window.click("deploy", cx);
        window.click("deploy", cx);
        assert!(status(window).starts_with("clicks=2 "), "{}", status(window));

        // A disabled dither button rejects activation.
        window.click("delete", cx);
        assert!(status(window).contains("deletes=0"), "{}", status(window));

        // The chart is a labelled figure.
        let chart = window.find("traffic");
        assert_eq!(chart.role(), Some(Role::Figure));
        assert!(chart.label().is_some_and(|l| l.contains("Desktop")));

        // Hovering the plot centre selects the middle data index.
        window.within("traffic").hover("plot", cx);
        assert!(status(window).contains("hover=Some(2)"), "{}", status(window));

        // Legend entries are Kit buttons; clicking toggles series selection.
        window.within("traffic").click(("legend-item", 1usize), cx);
        assert!(status(window).contains("selected=Some(1)"), "{}", status(window));
        window.within("traffic").click(("legend-item", 1usize), cx);
        assert!(status(window).contains("selected=None"), "{}", status(window));

        // Meter exposes a named progress indicator.
        let meter = window.find("upload");
        assert_eq!(meter.role(), Some(Role::ProgressIndicator));
        assert_eq!(meter.label(), Some("Upload"));
    })
    .unwrap();
}
