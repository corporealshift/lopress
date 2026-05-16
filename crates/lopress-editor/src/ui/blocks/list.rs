//! Read-only list rendering. Each item is a row of `[bullet/number] [runs]`.

use crate::model::types::ListItem;
use crate::ui::blocks::paragraph::render_paragraph;
use floem::views::{h_stack, text, v_stack_from_iter, Decorators};
use floem::IntoView;

pub fn render_list(ordered: bool, items: &[ListItem]) -> impl IntoView {
    let rows: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let prefix = if ordered {
                format!("{}.", idx + 1)
            } else {
                "•".to_string()
            };
            h_stack((
                text(prefix).style(|s| s.width(24.).font_size(15.)),
                render_paragraph(&item.runs),
            ))
            .style(|s| s.padding_vert(2.))
            .into_any()
        })
        .collect();
    v_stack_from_iter(rows).style(|s| s.padding_vert(4.).padding_left(8.))
}
