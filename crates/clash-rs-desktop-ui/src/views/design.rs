//! Design system primitives shared across pages.
//!
//! Single source of truth for typography, spacing, color usage, and reusable
//! composite elements. Pages should reach for these helpers instead of
//! constructing raw `border`/`vstack`/`text_block` chrome inline so that the
//! visual language stays consistent.

use std::time::{Duration, Instant};

use windows_reactor::*;

// ---------------------------------------------------------------------------
// Type scale
// ---------------------------------------------------------------------------
//
// One scale used everywhere, aligned to the WinUI 3 type ramp the framework
// renders natively (title 28 / subtitle 20 / body 14 / caption 12). Don't pick
// arbitrary `font_size` values in pages — reach for one of these.

pub(crate) mod type_scale {
    /// The single largest element on a page — the Overview status word.
    pub(crate) const HERO: f64 = 28.0;
    /// Big numeric readouts (metric cards).
    pub(crate) const METRIC: f64 = 22.0;
    /// Section and card titles.
    pub(crate) const TITLE: f64 = 20.0;
    /// Emphasised body (row titles, list emphasis).
    pub(crate) const BODY_STRONG: f64 = 14.0;
    /// Default body text.
    pub(crate) const BODY: f64 = 14.0;
    /// Secondary/supporting text, table cells, captions.
    pub(crate) const CAPTION: f64 = 12.0;
}

// ---------------------------------------------------------------------------
// Spacing scale (4-pt grid)
// ---------------------------------------------------------------------------

pub(crate) mod spacing {
    pub(crate) const XS: f64 = 4.0;
    pub(crate) const SM: f64 = 8.0;
    pub(crate) const MD: f64 = 12.0;
    /// The ubiquitous "between sections" gap that used to be hardcoded as 14.
    pub(crate) const MD_LG: f64 = 14.0;
    pub(crate) const LG: f64 = 16.0;
    /// Card padding.
    pub(crate) const XL: f64 = 20.0;
    pub(crate) const XXL: f64 = 24.0;
}

// ---------------------------------------------------------------------------
// Corner-radius scale — one set of radii so adjacent surfaces match
// ---------------------------------------------------------------------------

pub(crate) mod radius {
    /// Inset rows and small controls.
    pub(crate) const SM: f64 = 6.0;
    /// The default card/section/toolbar radius.
    pub(crate) const MD: f64 = 8.0;
    /// Prominent surfaces (the hero).
    pub(crate) const LG: f64 = 10.0;
}

/// Standard hairline border thickness for every card surface.
pub(crate) const HAIRLINE: f64 = 1.0;

// ---------------------------------------------------------------------------
// Section card — title + optional subtitle + content
// ---------------------------------------------------------------------------

pub(crate) fn section(
    title: impl Into<String>,
    subtitle: Option<&'static str>,
    content: impl Into<Element>,
) -> Element {
    let mut header_children: Vec<Element> = vec![
        text_block(title.into())
            .font_size(type_scale::TITLE)
            .semibold()
            .into(),
    ];
    if let Some(sub) = subtitle {
        header_children.push(
            text_block(sub)
                .font_size(type_scale::CAPTION)
                .foreground(tokens::SecondaryText)
                .into(),
        );
    }
    border(
        vstack((vstack(header_children).spacing(spacing::XS), content.into()))
            .spacing(spacing::MD),
    )
    .padding(Thickness::uniform(spacing::XL))
    .corner_radius(radius::MD)
    .background(tokens::CardBackground)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .into()
}

// ---------------------------------------------------------------------------
// Toolbar — slim horizontal strip used above content for actions/filters
// ---------------------------------------------------------------------------

pub(crate) fn toolbar(content: impl Into<Element>) -> Element {
    border(content.into())
        .padding(Thickness::xy(spacing::MD, spacing::SM))
        .corner_radius(radius::MD)
        .background(tokens::SubtleFill)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(HAIRLINE))
        .into()
}

// ---------------------------------------------------------------------------
// Selectable text — copyable output rendered via RichTextBlock
// ---------------------------------------------------------------------------
//
// Plain `text_block` (WinUI TextBlock) is never selectable, so command output
// and status text can't be copied. RichTextBlock supports text selection;
// multi-line strings become a single paragraph with explicit line breaks.

pub(crate) fn selectable_text(content: &str, font_size: f64) -> Element {
    use windows_reactor::core::rich_text::{RichTextBlock, RichTextInline, RichTextRun};

    let mut inlines = Vec::new();
    for (index, line) in content.split('\n').enumerate() {
        if index > 0 {
            inlines.push(RichTextInline::LineBreak);
        }
        inlines.push(RichTextInline::Run(RichTextRun::plain(line)));
    }
    RichTextBlock::single_paragraph(inlines)
        .font_size(font_size)
        .selectable()
        .wrap()
        .into()
}

// ---------------------------------------------------------------------------
// Metric card — single big number with label and caption
// ---------------------------------------------------------------------------

pub(crate) fn metric_card(label: &str, value: &str, caption: &str) -> Element {
    border(
        vstack((
            text_block(label.to_owned())
                .font_size(type_scale::CAPTION)
                .bold()
                .foreground(tokens::SecondaryText),
            text_block(value.to_owned())
                .font_size(type_scale::METRIC)
                .bold(),
            text_block(caption.to_owned())
                .font_size(type_scale::CAPTION)
                .foreground(tokens::TertiaryText),
        ))
        .spacing(spacing::XS),
    )
    .padding(Thickness::uniform(spacing::LG))
    .corner_radius(radius::MD)
    .background(tokens::SubtleFill)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .min_width(140.0)
    .horizontal_alignment(HorizontalAlignment::Stretch)
    .into()
}

/// Lay metric cards out in a single row of equal-width (star-sized) columns so
/// the strip fills the available width and the cards shrink together instead of
/// overflowing off the right edge on a narrow window (the old fixed-width
/// `hstack` couldn't give). Cards keep a `min_width` floor.
pub(crate) fn metric_grid(cards: Vec<Element>) -> Element {
    if cards.is_empty() {
        return Element::Empty;
    }
    let columns: Vec<GridLength> = std::iter::repeat_n(GridLength::STAR, cards.len()).collect();
    let placed: Vec<Element> = cards
        .into_iter()
        .enumerate()
        .map(|(index, card)| card.grid_column(index as i32))
        .collect();
    grid(placed)
        .columns(columns)
        .column_spacing(spacing::MD)
        .into()
}

// ---------------------------------------------------------------------------
// Empty state — icon-less but distinct: headline + description + optional CTA
// ---------------------------------------------------------------------------

pub(crate) fn empty_state(
    headline: impl Into<String>,
    description: impl Into<String>,
    cta: Option<Element>,
) -> Element {
    let mut children: Vec<Element> = vec![
        text_block(headline.into())
            .font_size(type_scale::TITLE)
            .semibold()
            .into(),
        text_block(description.into())
            .font_size(type_scale::BODY)
            .foreground(tokens::SecondaryText)
            .into(),
    ];
    if let Some(action) = cta {
        children.push(action);
    }
    border(vstack(children).spacing(spacing::SM))
        .padding(Thickness::uniform(spacing::XXL))
        .corner_radius(radius::MD)
        .background(tokens::SubtleFill)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(HAIRLINE))
        .into()
}

// ---------------------------------------------------------------------------
// Settings row — title + description on the left, control on the right
// ---------------------------------------------------------------------------
//
// Mirrors the layout used by 1Password / macOS / Windows 11 settings:
// the user reads what the row does, then reaches for the control.

pub(crate) fn settings_row(
    title: impl Into<String>,
    description: impl Into<String>,
    control: Element,
) -> Element {
    border(
        hstack((
            vstack((
                text_block(title.into())
                    .font_size(type_scale::BODY_STRONG)
                    .bold(),
                text_block(description.into())
                    .font_size(type_scale::CAPTION)
                    .foreground(tokens::SecondaryText),
            ))
            .spacing(2.0)
            .width(360.0),
            control,
        ))
        .spacing(spacing::LG),
    )
    .padding(Thickness::xy(spacing::MD, spacing::SM + 2.0))
    .corner_radius(radius::SM)
    .background(tokens::SubtleFill)
    .into()
}

// ---------------------------------------------------------------------------
// Status badge — single accent pill, color depends on Tone
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Success,
    Critical,
}

impl Tone {
    fn brush(self) -> ThemeRef {
        match self {
            Tone::Success => tokens::SystemSuccess,
            Tone::Critical => tokens::SystemCritical,
        }
    }

    fn severity(self) -> InfoBarSeverity {
        match self {
            Tone::Success => InfoBarSeverity::Success,
            Tone::Critical => InfoBarSeverity::Error,
        }
    }

    pub(crate) fn from_ok(ok: bool) -> Self {
        if ok { Tone::Success } else { Tone::Critical }
    }
}

pub(crate) fn status_pill(label: impl Into<String>, tone: Tone) -> Element {
    let brush = tone.brush();
    border(
        text_block(label.into())
            .font_size(type_scale::BODY)
            .bold()
            .foreground(tokens::CardBackground),
    )
    .padding(Thickness::xy(spacing::MD, 6.0))
    .corner_radius(radius::MD)
    .background(brush.clone())
    .border_brush(brush)
    .border_thickness(Thickness::uniform(HAIRLINE))
    .into()
}

// ---------------------------------------------------------------------------
// Hero card — the prominent status surface at the top of Overview
// ---------------------------------------------------------------------------
//
// A single large status word with a supporting caption on the left and a
// right-aligned action cluster, on a star-grid so the actions hold the right
// edge instead of crowding the title on a narrow window. Replaces the bespoke
// `border()` chrome the Overview used to hand-build.

pub(crate) fn hero_card(
    status: impl Into<String>,
    status_color: ThemeRef,
    detail: impl Into<String>,
    actions: Element,
) -> Element {
    let detail = detail.into();
    let mut left: Vec<Element> = vec![
        text_block(status.into())
            .font_size(type_scale::HERO)
            .semibold()
            .foreground(status_color)
            .into(),
    ];
    if !detail.is_empty() {
        left.push(
            text_block(detail)
                .font_size(type_scale::CAPTION)
                .foreground(tokens::SecondaryText)
                .into(),
        );
    }

    let left_col: Element = vstack(left).spacing(spacing::XS).grid_column(0).into();
    let action_col: Element = actions
        .horizontal_alignment(HorizontalAlignment::Right)
        .grid_column(1);

    let row = grid(vec![left_col, action_col])
        .columns([GridLength::STAR, GridLength::Auto])
        .horizontal_alignment(HorizontalAlignment::Stretch);

    border(row)
        .padding(Thickness::uniform(spacing::XL))
        .corner_radius(radius::LG)
        .background(tokens::CardBackground)
        .border_brush(tokens::CardStroke)
        .border_thickness(Thickness::uniform(HAIRLINE))
        .into()
}

// ---------------------------------------------------------------------------
// Toast — single transient notification rendered by the app shell
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
pub(crate) struct Toast {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) tone: Tone,
    pub(crate) created_at: Instant,
}

impl Toast {
    pub(crate) fn new(
        id: u64,
        title: impl Into<String>,
        message: impl Into<String>,
        tone: Tone,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            message: message.into(),
            tone,
            created_at: Instant::now(),
        }
    }

    pub(crate) fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) > Duration::from_secs(6)
    }
}

#[derive(Clone)]
pub(crate) struct ToastQueue {
    pub(crate) toasts: Vec<Toast>,
    pub(crate) set_toasts: AsyncSetState<Vec<Toast>>,
    pub(crate) next_id: AsyncSetState<u64>,
    pub(crate) next_id_value: u64,
}

impl ToastQueue {
    pub(crate) fn push(
        &self,
        title: impl Into<String>,
        message: impl Into<String>,
        tone: Tone,
    ) {
        let id = self.next_id_value;
        self.next_id.call(id.wrapping_add(1));
        let toast = Toast::new(id, title, message, tone);
        let mut next = self.toasts.clone();
        next.push(toast);
        self.set_toasts.call(next);
    }

    pub(crate) fn dismiss(&self, id: u64) {
        let next = self
            .toasts
            .iter()
            .filter(|t| t.id != id)
            .cloned()
            .collect::<Vec<_>>();
        self.set_toasts.call(next);
    }

    pub(crate) fn purge_expired(&self) {
        let now = Instant::now();
        let next = self
            .toasts
            .iter()
            .filter(|t| !t.is_expired(now))
            .cloned()
            .collect::<Vec<_>>();
        if next.len() != self.toasts.len() {
            self.set_toasts.call(next);
        }
    }
}

pub(crate) fn toast_card(toast: &Toast, queue: &ToastQueue) -> Element {
    let toast_id = toast.id;
    let queue_dismiss = queue.clone();
    let mut bar = InfoBar::new(toast.title.clone())
        .severity(toast.tone.severity())
        .is_open(true)
        .is_closable(true)
        .on_close(move || queue_dismiss.dismiss(toast_id));
    if !toast.message.is_empty() {
        bar = bar.message(toast.message.clone());
    }
    bar.into()
}

pub(crate) fn toast_stack(queue: &ToastQueue) -> Element {
    if queue.toasts.is_empty() {
        return Element::Empty;
    }
    let cards = queue
        .toasts
        .iter()
        .map(|toast| toast_card(toast, queue))
        .collect::<Vec<_>>();
    vstack(cards).spacing(spacing::SM).into()
}

// ---------------------------------------------------------------------------
// Busy strip — thin progress indicator at the very top of the workspace
// ---------------------------------------------------------------------------

pub(crate) fn busy_strip(busy: bool) -> Element {
    if !busy {
        return border(Element::Empty)
            .height(2.0)
            .background(tokens::CardStroke)
            .into();
    }
    border(Element::Empty)
        .height(2.0)
        .background(tokens::SystemAttention)
        .into()
}
