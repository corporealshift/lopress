use floem::IntoView;
use floem::views::label;

pub fn root_view() -> impl IntoView {
    label(|| "lopress")
}
