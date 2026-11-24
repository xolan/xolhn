use std::sync::{Arc, OnceLock};

use super::{
    ActionCallback, BoolCallback, FontSizeCallback, LineHeightCallback, OptionalTextCallback,
    ThemeCallback,
};

use gpui::{
    AnyElement, App, InteractiveElement, IntoElement, ParentElement, ScrollHandle,
    StatefulInteractiveElement, Styled, UniformListScrollHandle, div, prelude::FluentBuilder, px,
    relative, uniform_list,
};
use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, StyledExt, h_flex, v_flex,
};
use xolhn_core::html::{ContentBlock, TextSpan, parse_hn_html};
use xolhn_core::settings::{
    AppSettings, DEFAULT_FONT_SIZE_PX, LineHeightDensity, MAX_FONT_SIZE_PX, MIN_FONT_SIZE_PX,
};
use xolhn_core::theme::ThemeId;

use crate::fonts::{BUNDLED_MONO_FONTS, BUNDLED_UI_FONTS};
use crate::smooth_scroll::{SmoothScrollState, SmoothScrollTarget, smooth_scroll};
use crate::theme_bridge::{UiColors, rgb_hex};

fn cached_preview_blocks() -> &'static [ContentBlock] {
    static PREVIEW_BLOCKS: OnceLock<Vec<ContentBlock>> = OnceLock::new();
    PREVIEW_BLOCKS.get_or_init(|| parse_hn_html(SAMPLE_PREVIEW_HTML))
}

const FONT_SIZE_PRESETS: [f32; 4] = [13.0, 15.0, 17.0, 19.0];

const SAMPLE_PREVIEW_HTML: &str = concat!(
    "<p>Native <b>GPUI</b> rendering with <code>zero-regex</code> HTML parsing ",
    "and instant theme switching. Read discussions on ",
    "<a href=\"https://news.ycombinator.com\">news.ycombinator.com</a>.</p>",
    "<pre>fn line_height(density: LineHeightDensity) -> f32 {\n",
    "    density.multiplier()\n",
    "}</pre>",
);

/// Bundled scroll handles and smooth-scroll animation states for the settings panel.
pub struct SettingsScrollHandles<'a> {
    pub scroll_handle: &'a ScrollHandle,
    pub scroll_smooth: &'a SmoothScrollState,
    pub ui_fonts_scroll: &'a UniformListScrollHandle,
    pub ui_fonts_smooth: &'a SmoothScrollState,
    pub mono_fonts_scroll: &'a UniformListScrollHandle,
    pub mono_fonts_smooth: &'a SmoothScrollState,
}

/// Interactive callbacks wired into the settings panel controls.
#[derive(Clone)]
pub struct SettingsCallbacks {
    pub on_select_theme: ThemeCallback,
    pub on_set_font_size: FontSizeCallback,
    pub on_set_line_height: LineHeightCallback,
    pub on_set_ui_font: OptionalTextCallback,
    pub on_set_mono_font: OptionalTextCallback,
    pub on_set_smooth_scrolling: BoolCallback,
    pub on_clear_cache: ActionCallback,
    pub on_reset_typography: ActionCallback,
    pub on_close: Option<ActionCallback>,
}

/// Renders the Appearance & Accessibility settings drawer/tab view.
#[allow(clippy::too_many_arguments)]
pub fn render_settings_panel(
    settings: &AppSettings,
    ui_fonts: Arc<Vec<String>>,
    mono_fonts: Arc<Vec<String>>,
    colors: &UiColors,
    handles: SettingsScrollHandles<'_>,
    is_mobile: bool,
    callbacks: SettingsCallbacks,
    cx: &App,
) -> AnyElement {
    let close_callback = if is_mobile {
        None
    } else {
        callbacks.on_close.clone()
    };

    let header = h_flex()
        .id("settings-header")
        .w_full()
        .flex_shrink_0()
        .px_4()
        .py_3()
        .justify_between()
        .items_center()
        .border_b_1()
        .border_color(colors.border)
        .bg(colors.surface)
        .child(
            h_flex().gap_2().items_center().child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(colors.text)
                    .child("Appearance & Accessibility"),
            ),
        )
        .when_some(close_callback, |this, on_close| {
            this.child(
                Button::new("settings-close-btn")
                    .icon(IconName::Close)
                    .ghost()
                    .small()
                    .accessibility_label("Close settings")
                    .on_click(move |_, window, cx| on_close(window, cx)),
            )
        });

    let body_padding = if is_mobile { px(14.) } else { px(16.) };

    let scroll_content = v_flex()
        .id("settings-scroll-body")
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_y_scroll()
        .track_scroll(handles.scroll_handle)
        .child(
            v_flex()
                .w_full()
                .flex_shrink_0()
                .p(body_padding)
                .gap_5()
                .child(render_theme_suite_section(settings, colors, &callbacks))
                .child(render_typography_section(
                    settings, ui_fonts, mono_fonts, &handles, colors, &callbacks,
                ))
                .child(render_scrolling_section(settings, colors, &callbacks))
                .child(render_live_preview_section(settings, colors, cx))
                .child(render_storage_and_reset_section(colors, &callbacks)),
        );

    let smooth_body = smooth_scroll(
        scroll_content,
        SmoothScrollTarget::Scroll(handles.scroll_handle.clone()),
        handles.scroll_smooth,
        settings.smooth_scrolling,
    );

    let scrollable_body = v_flex()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .child(smooth_body)
        .vertical_scrollbar(handles.scroll_handle);

    v_flex()
        .id("settings-panel")
        .size_full()
        .bg(colors.surface)
        .text_color(colors.text)
        .child(header)
        .child(scrollable_body)
        .into_any_element()
}

fn render_section_header(
    title: &'static str,
    subtitle: &'static str,
    colors: &UiColors,
) -> gpui::Div {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_xs()
                .font_semibold()
                .text_color(colors.text)
                .child(title),
        )
        .child(div().text_xs().text_color(colors.muted).child(subtitle))
}

fn render_theme_suite_section(
    settings: &AppSettings,
    colors: &UiColors,
    callbacks: &SettingsCallbacks,
) -> gpui::Div {
    let mut rows = v_flex().w_full().gap_2();

    for pair in ThemeId::ALL.chunks(2) {
        let mut row = h_flex().w_full().gap_2().items_stretch();
        for &theme_id in pair {
            row = row.child(render_theme_card(
                theme_id,
                settings.theme == theme_id,
                colors,
                callbacks.on_select_theme.clone(),
            ));
        }
        rows = rows.child(row);
    }

    v_flex()
        .w_full()
        .gap_2p5()
        .p_3()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .child(render_section_header(
            "Theme Suite",
            "Choose a high-contrast dark or light palette with depth rail colors.",
            colors,
        ))
        .child(rows)
}

fn render_theme_card(
    theme_id: ThemeId,
    is_active: bool,
    active_colors: &UiColors,
    on_select_theme: ThemeCallback,
) -> BaseButton {
    let palette = theme_id.palette();
    let theme_name = theme_id.display_name();
    let card_bg = rgb_hex(palette.surface);
    let preview_bg = rgb_hex(palette.bg);
    let card_text = rgb_hex(palette.text);
    let card_muted = rgb_hex(palette.muted);
    let card_accent = rgb_hex(palette.accent);
    let card_border = if is_active {
        active_colors.accent
    } else {
        rgb_hex(palette.border)
    };

    let mut swatches = h_flex()
        .gap_1()
        .items_center()
        .child(
            div()
                .size(px(8.))
                .rounded(px(4.))
                .bg(preview_bg)
                .border_1()
                .border_color(card_muted),
        )
        .child(div().size(px(8.)).rounded(px(4.)).bg(card_accent));

    for rail_hex in palette.rails {
        swatches = swatches.child(div().size(px(7.)).rounded(px(3.5)).bg(rgb_hex(rail_hex)));
    }

    let mini_preview = h_flex()
        .w_full()
        .h(px(28.))
        .px_2()
        .justify_between()
        .items_center()
        .rounded(px(5.))
        .bg(preview_bg)
        .border_1()
        .border_color(rgb_hex(palette.border))
        .child(swatches)
        .when(is_active, |this| {
            this.child(Icon::new(IconName::Check).xsmall().text_color(card_accent))
        });

    let label_row = h_flex()
        .w_full()
        .justify_between()
        .items_center()
        .gap_1()
        .child(
            div()
                .min_w_0()
                .text_xs()
                .font_semibold()
                .text_color(card_text)
                .truncate()
                .child(theme_name),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(card_muted)
                .child(if palette.is_dark { "Dark" } else { "Light" }),
        );

    BaseButton::new(gpui::SharedString::from(format!(
        "theme-card-{}",
        theme_id.slug()
    )))
    .selected(is_active)
    .accessibility_label(format!("Theme: {theme_name}"))
    .flex_1()
    .min_w(px(124.))
    .p_2()
    .rounded(px(7.))
    .bg(card_bg)
    .border_2()
    .border_color(card_border)
    .cursor_pointer()
    .hover(move |style| style.border_color(card_accent))
    .on_click(move |_, window, cx| on_select_theme(theme_id, window, cx))
    .child(
        v_flex()
            .w_full()
            .gap_1p5()
            .child(mini_preview)
            .child(label_row),
    )
}

fn render_typography_section(
    settings: &AppSettings,
    ui_fonts: Arc<Vec<String>>,
    mono_fonts: Arc<Vec<String>>,
    handles: &SettingsScrollHandles<'_>,
    colors: &UiColors,
    callbacks: &SettingsCallbacks,
) -> gpui::Div {
    let clamped_size = clamp_font_size(settings.base_font_size_px);
    let can_decrement = clamped_size > MIN_FONT_SIZE_PX + f32::EPSILON;
    let can_increment = clamped_size < MAX_FONT_SIZE_PX - f32::EPSILON;

    let dec_cb = callbacks.on_set_font_size.clone();
    let inc_cb = callbacks.on_set_font_size.clone();
    let dec_target = clamp_font_size(clamped_size - 1.0);
    let inc_target = clamp_font_size(clamped_size + 1.0);

    let stepper_controls = h_flex()
        .gap_2()
        .items_center()
        .child(
            Button::new("settings-font-size-dec")
                .label("-")
                .small()
                .outline()
                .disabled(!can_decrement)
                .on_click(move |_, window, cx| dec_cb(dec_target, window, cx)),
        )
        .child(
            div()
                .min_w(px(54.))
                .px_2()
                .py_1()
                .rounded(px(5.))
                .bg(colors.bg)
                .border_1()
                .border_color(colors.border)
                .text_xs()
                .font_semibold()
                .text_color(colors.text)
                .flex()
                .justify_center()
                .items_center()
                .child(format!("{clamped_size:.0} px")),
        )
        .child(
            Button::new("settings-font-size-inc")
                .label("+")
                .small()
                .outline()
                .disabled(!can_increment)
                .on_click(move |_, window, cx| inc_cb(inc_target, window, cx)),
        );

    let mut preset_pills = h_flex().gap_1p5().flex_wrap().items_center();
    for preset in FONT_SIZE_PRESETS {
        let is_active = (clamped_size - preset).abs() < 0.25;
        let on_set_size = callbacks.on_set_font_size.clone();
        preset_pills = preset_pills.child(
            Button::new(("settings-font-preset", preset as u32))
                .label(format!("{preset:.0}px"))
                .xsmall()
                .selected(is_active)
                .when(is_active, |btn| btn.primary())
                .when(!is_active, |btn| btn.outline())
                .on_click(move |_, window, cx| on_set_size(preset, window, cx)),
        );
    }

    let font_size_block = v_flex()
        .w_full()
        .gap_2()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(colors.text)
                        .child("Base Font Size (12px – 22px)"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(colors.muted)
                        .child(format!("Default {DEFAULT_FONT_SIZE_PX:.0}px")),
                ),
        )
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(stepper_controls)
                .child(preset_pills),
        );

    let mut density_row = h_flex().w_full().gap_1p5().flex_wrap();
    for density in LineHeightDensity::ALL {
        let is_selected = settings.line_height_density == density;
        let on_set_line_height = callbacks.on_set_line_height.clone();
        density_row = density_row.child(
            Button::new(gpui::SharedString::from(format!(
                "settings-line-height-{}",
                density.label()
            )))
            .label(density.label())
            .small()
            .selected(is_selected)
            .when(is_selected, |btn| btn.primary())
            .when(!is_selected, |btn| btn.outline())
            .on_click(move |_, window, cx| on_set_line_height(density, window, cx)),
        );
    }

    let line_height_block = v_flex()
        .w_full()
        .gap_1p5()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(colors.text)
                        .child("Line-Height Density"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(colors.muted)
                        .child(format!("{:.2}×", settings.line_height_density.multiplier())),
                ),
        )
        .child(density_row);

    let ui_font_block = render_font_picker(
        FontPickerSpec {
            id_prefix: "ui",
            title: "UI / Reading Font Family",
            default_label: "System Default",
            bundled: BUNDLED_UI_FONTS,
            list_height: px(170.),
            aria_prefix: "UI font",
        },
        settings.ui_font_family.as_deref(),
        ui_fonts,
        handles.ui_fonts_scroll,
        handles.ui_fonts_smooth,
        settings.smooth_scrolling,
        colors,
        callbacks.on_set_ui_font.clone(),
    );

    let mono_font_block = render_font_picker(
        FontPickerSpec {
            id_prefix: "mono",
            title: "Monospace Font Family",
            default_label: "System Mono",
            bundled: BUNDLED_MONO_FONTS,
            list_height: px(140.),
            aria_prefix: "Mono font",
        },
        settings.mono_font_family.as_deref(),
        mono_fonts,
        handles.mono_fonts_scroll,
        handles.mono_fonts_smooth,
        settings.smooth_scrolling,
        colors,
        callbacks.on_set_mono_font.clone(),
    );

    v_flex()
        .w_full()
        .gap_4()
        .p_3()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .child(render_section_header(
            "Typography & Reading Accessibility",
            "Customize font scale, line spacing, and bundled or system font families.",
            colors,
        ))
        .child(font_size_block)
        .child(line_height_block)
        .child(ui_font_block)
        .child(mono_font_block)
}

struct FontPickerSpec {
    id_prefix: &'static str,
    title: &'static str,
    default_label: &'static str,
    bundled: &'static [&'static str],
    list_height: gpui::Pixels,
    aria_prefix: &'static str,
}

#[allow(clippy::too_many_arguments)]
fn render_font_picker(
    spec: FontPickerSpec,
    selected_font: Option<&str>,
    fonts: Arc<Vec<String>>,
    scroll_handle: &UniformListScrollHandle,
    smooth_state: &SmoothScrollState,
    smooth_scrolling: bool,
    colors: &UiColors,
    on_select: OptionalTextCallback,
) -> gpui::Div {
    let is_system_default = selected_font.is_none();
    let on_default = on_select.clone();
    let id_prefix = spec.id_prefix;

    let mut quick_pills = h_flex().w_full().gap_1p5().flex_wrap().child(
        Button::new(gpui::SharedString::from(format!(
            "settings-{id_prefix}-font-default"
        )))
        .label(spec.default_label)
        .xsmall()
        .selected(is_system_default)
        .when(is_system_default, |btn| btn.primary())
        .when(!is_system_default, |btn| btn.outline())
        .on_click(move |_, window, cx| on_default(None, window, cx)),
    );

    for &bundled_name in spec.bundled {
        let resolved_family = resolve_loaded_font_family(bundled_name, &fonts);
        let is_selected =
            is_font_family_selected(selected_font, &resolved_family, Some(bundled_name));
        let on_pick = on_select.clone();

        quick_pills = quick_pills.child(
            Button::new(gpui::SharedString::from(format!(
                "settings-{id_prefix}-font-bundled-{bundled_name}"
            )))
            .label(bundled_name)
            .xsmall()
            .selected(is_selected)
            .when(is_selected, |btn| btn.primary())
            .when(!is_selected, |btn| btn.outline())
            .on_click(move |_, window, cx| {
                on_pick(Some(resolved_family.clone()), window, cx);
            }),
        );
    }

    let font_entries: Arc<Vec<String>> = if fonts.is_empty() {
        Arc::new(
            spec.bundled
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        )
    } else {
        fonts
    };

    let list_colors = *colors;
    let selected_owned = selected_font.map(str::to_string);
    let aria_prefix = spec.aria_prefix;
    let font_count = font_entries.len();

    let virtual_list = uniform_list(
        gpui::SharedString::from(format!("settings-{id_prefix}-fonts-list")),
        font_count,
        move |range, _window, _cx| {
            let mut rows = Vec::with_capacity(range.len());
            for idx in range {
                let Some(font_name) = font_entries.get(idx) else {
                    continue;
                };
                let is_selected =
                    is_font_family_selected(selected_owned.as_deref(), font_name, None);
                let font_value = font_name.clone();
                let font_label = font_name.clone();
                let on_pick = on_select.clone();
                let row_hover = if is_selected {
                    list_colors.accent_soft
                } else {
                    list_colors.hover
                };

                rows.push(
                    BaseButton::new(gpui::SharedString::from(format!(
                        "settings-{id_prefix}-font-row-{idx}"
                    )))
                    .selected(is_selected)
                    .accessibility_label(format!("{aria_prefix}: {font_label}"))
                    .w_full()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .px_2p5()
                    .rounded(px(4.))
                    .bg(if is_selected {
                        list_colors.accent_soft
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .hover(move |style| style.bg(row_hover))
                    .on_click(move |_, window, cx| on_pick(Some(font_value.clone()), window, cx))
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .text_xs()
                                    .text_color(if is_selected {
                                        list_colors.accent
                                    } else {
                                        list_colors.text
                                    })
                                    .when(is_selected, |d| d.font_semibold())
                                    .truncate()
                                    .child(font_label),
                            )
                            .when(is_selected, |this| {
                                this.child(
                                    Icon::new(IconName::Check)
                                        .xsmall()
                                        .text_color(list_colors.accent),
                                )
                            }),
                    ),
                );
            }
            rows
        },
    )
    .track_scroll(scroll_handle)
    .size_full()
    .p_1();

    let smooth_virtual_list = smooth_scroll(
        virtual_list,
        SmoothScrollTarget::UniformList(scroll_handle.clone()),
        smooth_state,
        smooth_scrolling,
    );

    let font_list = div()
        .relative()
        .w_full()
        .h(spec.list_height)
        .rounded(px(6.))
        .bg(colors.bg)
        .border_1()
        .border_color(colors.border)
        .overflow_hidden()
        .child(smooth_virtual_list)
        .vertical_scrollbar(scroll_handle);

    v_flex()
        .w_full()
        .gap_1p5()
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .child(
                    div()
                        .text_xs()
                        .font_medium()
                        .text_color(colors.text)
                        .child(spec.title),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(colors.muted)
                        .truncate()
                        .child(selected_font.unwrap_or(spec.default_label).to_string()),
                ),
        )
        .child(quick_pills)
        .child(font_list)
}

fn render_scrolling_section(
    settings: &AppSettings,
    colors: &UiColors,
    callbacks: &SettingsCallbacks,
) -> gpui::Div {
    let on_toggle = callbacks.on_set_smooth_scrolling.clone();

    v_flex()
        .w_full()
        .gap_2p5()
        .p_3()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .child(render_section_header(
            "Scrolling & Motion",
            "Tune scroll feel for mouse wheels across feeds and comment threads.",
            colors,
        ))
        .child(
            h_flex()
                .w_full()
                .p_2p5()
                .gap_3()
                .items_center()
                .justify_between()
                .rounded(px(6.))
                .bg(colors.bg)
                .border_1()
                .border_color(colors.border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(colors.text)
                                .child("Smooth Scrolling"),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(colors.muted)
                                .child("Animate mouse-wheel scrolling (trackpad and touch stay 1:1; respects Reduce Motion)."),
                        ),
                )
                .child(
                    Switch::new("settings-smooth-scrolling")
                        .checked(settings.smooth_scrolling)
                        .accessibility_label("Smooth scrolling")
                        .on_change(move |checked, window, cx| on_toggle(*checked, window, cx)),
                ),
        )
}

fn render_live_preview_section(settings: &AppSettings, colors: &UiColors, cx: &App) -> gpui::Div {
    let base_px = clamp_font_size(settings.base_font_size_px);
    let mono_px = (base_px - 1.5).max(11.0);
    let line_height_mult = settings.line_height_density.multiplier();

    let ui_font = settings
        .ui_font_family
        .clone()
        .unwrap_or_else(|| cx.theme().font_family.to_string());
    let mono_font = settings
        .mono_font_family
        .clone()
        .unwrap_or_else(|| cx.theme().mono_font_family.to_string());

    let mut rendered_blocks = v_flex().w_full().gap_2();

    for block in cached_preview_blocks() {
        match block {
            ContentBlock::Paragraph(spans) | ContentBlock::Quote(spans) => {
                rendered_blocks = rendered_blocks.child(render_preview_spans(
                    spans,
                    base_px,
                    mono_px,
                    line_height_mult,
                    &ui_font,
                    &mono_font,
                    colors,
                ));
            }
            ContentBlock::CodeBlock(code) => {
                rendered_blocks = rendered_blocks.child(
                    div()
                        .w_full()
                        .p_2p5()
                        .rounded(px(6.))
                        .bg(colors.code_bg)
                        .border_1()
                        .border_color(colors.border)
                        .font_family(mono_font.clone())
                        .text_size(px(mono_px))
                        .line_height(relative(line_height_mult))
                        .text_color(colors.text)
                        .child(code.clone()),
                );
            }
        }
    }

    let sample_card =
        h_flex()
            .id("settings-live-preview-card")
            .w_full()
            .items_stretch()
            .rounded(px(7.))
            .bg(colors.bg)
            .border_1()
            .border_color(colors.border)
            .overflow_hidden()
            .child(div().w(px(3.5)).flex_shrink_0().bg(colors.rails[0]))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .p_3()
                    .gap_2()
                    .font_family(ui_font)
                    .child(
                        h_flex()
                            .w_full()
                            .justify_between()
                            .items_center()
                            .gap_2()
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px((base_px - 1.5).max(11.0)))
                                            .font_semibold()
                                            .text_color(colors.text)
                                            .child("xolan"),
                                    )
                                    .child(
                                        div()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded(px(4.))
                                            .bg(colors.accent_soft)
                                            .text_size(px(10.))
                                            .font_bold()
                                            .text_color(colors.accent)
                                            .child("OP"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px((base_px - 2.0).max(10.0)))
                                            .text_color(colors.subtle)
                                            .child("14m ago"),
                                    ),
                            )
                            .child(div().text_size(px(10.)).text_color(colors.muted).child(
                                format!(
                                    "{base_px:.0}px • {}",
                                    settings.line_height_density.label()
                                ),
                            )),
                    )
                    .child(rendered_blocks),
            );

    v_flex()
        .w_full()
        .gap_2p5()
        .p_3()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .child(render_section_header(
            "Live Reading Preview",
            "Preview how comments, inline formatting, and code blocks render.",
            colors,
        ))
        .child(sample_card)
}

fn render_preview_spans(
    spans: &[TextSpan],
    base_px: f32,
    mono_px: f32,
    line_height_mult: f32,
    ui_font: &str,
    mono_font: &str,
    colors: &UiColors,
) -> gpui::Div {
    let mut row = h_flex()
        .w_full()
        .flex_wrap()
        .items_center()
        .gap_x_1()
        .gap_y_0p5()
        .font_family(ui_font.to_string())
        .text_size(px(base_px))
        .line_height(relative(line_height_mult))
        .text_color(colors.text);

    for span in spans {
        if span.code {
            row = row.child(
                div()
                    .px_1()
                    .rounded(px(3.))
                    .bg(colors.code_bg)
                    .border_1()
                    .border_color(colors.border)
                    .font_family(mono_font.to_string())
                    .text_size(px(mono_px))
                    .text_color(colors.accent)
                    .child(span.text.trim().to_string()),
            );
        } else if span.link.is_some() {
            row = row.child(
                div()
                    .text_color(colors.link)
                    .underline()
                    .when(span.bold, |d| d.font_bold())
                    .when(span.italic, |d| d.italic())
                    .child(span.text.trim().to_string()),
            );
        } else {
            for word in span.text.split_whitespace() {
                row = row.child(
                    div()
                        .when(span.bold, |d| d.font_bold())
                        .when(span.italic, |d| d.italic())
                        .child(word.to_string()),
                );
            }
        }
    }

    row
}

fn render_storage_and_reset_section(colors: &UiColors, callbacks: &SettingsCallbacks) -> gpui::Div {
    let on_reset = callbacks.on_reset_typography.clone();
    let on_clear = callbacks.on_clear_cache.clone();

    v_flex()
        .w_full()
        .gap_3()
        .p_3()
        .rounded(px(8.))
        .bg(colors.surface_2)
        .border_1()
        .border_color(colors.border)
        .child(render_section_header(
            "Storage & Reset",
            "Restore default typography or purge cached Hacker News responses.",
            colors,
        ))
        .child(
            h_flex()
                .w_full()
                .gap_2()
                .flex_wrap()
                .child(
                    Button::new("settings-reset-typography")
                        .label("Reset Typography")
                        .small()
                        .outline()
                        .on_click(move |_, window, cx| on_reset(window, cx)),
                )
                .child(
                    Button::new("settings-clear-cache")
                        .label("Clear API Cache")
                        .small()
                        .outline()
                        .on_click(move |_, window, cx| on_clear(window, cx)),
                ),
        )
        .child(
            div()
                .pt_1()
                .border_t_1()
                .border_color(colors.border)
                .text_xs()
                .text_color(colors.subtle)
                .child("XolHN v1.0.0 • Read-only Hacker News client (Algolia API)"),
        )
}

fn clamp_font_size(size_px: f32) -> f32 {
    if size_px.is_finite() {
        size_px.clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX)
    } else {
        DEFAULT_FONT_SIZE_PX
    }
}

/// Resolves a bundled font name against the loaded font family list (`ui_fonts` or `mono_fonts`).
///
/// Prefers exact case-insensitive matches, then case-insensitive prefix matches,
/// falling back to `bundled_name` when absent.
pub fn resolve_loaded_font_family(bundled_name: &str, available_fonts: &[String]) -> String {
    let target_lower = bundled_name.trim().to_lowercase();

    if let Some(exact) = available_fonts
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty() && s.to_lowercase() == target_lower)
    {
        return exact.to_string();
    }

    if let Some(prefix) = available_fonts
        .iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty() && s.to_lowercase().starts_with(&target_lower))
    {
        return prefix.to_string();
    }

    bundled_name.to_string()
}

fn is_font_family_selected(
    selected_font: Option<&str>,
    candidate_family: &str,
    bundled_alias: Option<&str>,
) -> bool {
    let Some(current) = selected_font.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    if current.eq_ignore_ascii_case(candidate_family.trim()) {
        return true;
    }
    bundled_alias.is_some_and(|alias| current.eq_ignore_ascii_case(alias.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_bundled_font_exact_and_prefix_from_loaded_list() {
        let loaded = vec![
            "inter".to_string(),
            "Atkinson Hyperlegible Next".to_string(),
            "Roboto".to_string(),
        ];

        assert_eq!(resolve_loaded_font_family("Inter", &loaded), "inter");
        assert_eq!(
            resolve_loaded_font_family("Atkinson Hyperlegible", &loaded),
            "Atkinson Hyperlegible Next"
        );
        assert_eq!(resolve_loaded_font_family("Literata", &loaded), "Literata");
    }

    #[test]
    fn matches_selected_font_case_insensitively_and_with_alias() {
        assert!(!is_font_family_selected(None, "Inter", Some("Inter")));
        assert!(is_font_family_selected(
            Some("inter"),
            "Inter",
            Some("Inter")
        ));
        assert!(is_font_family_selected(
            Some("Atkinson Hyperlegible"),
            "Atkinson Hyperlegible Next",
            Some("Atkinson Hyperlegible")
        ));
        assert!(!is_font_family_selected(
            Some("Roboto"),
            "Inter",
            Some("Inter")
        ));
    }

    #[test]
    fn clamps_font_size_to_supported_range() {
        assert_eq!(clamp_font_size(8.0), MIN_FONT_SIZE_PX);
        assert_eq!(clamp_font_size(15.0), 15.0);
        assert_eq!(clamp_font_size(30.0), MAX_FONT_SIZE_PX);
        assert_eq!(clamp_font_size(f32::NAN), DEFAULT_FONT_SIZE_PX);
    }
}
