# vizia_core 0.4.0, patched for Shor

Upstream vizia_core 0.4.0 plus one feature: a user zoom.

`WindowDescription::user_scale_factor` exists upstream but nothing applies
it. This patch keeps the screen's scale (`Style::system_scale`) and the
user's zoom (`Style::user_scale`) apart, makes `dpi_factor` their product
everywhere it is set (`BackendContext::add_main_window`,
`BackendContext::set_scale_factor`), and adds
`EventContext::set_user_scale`, which relayouts, restyles and redraws the
window the way a screen scale change does.

Every change is marked `SHOR PATCH`. To update vizia, re-apply those.
