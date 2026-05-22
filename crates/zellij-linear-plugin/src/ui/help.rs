use crate::ui::text::truncate;

pub fn render(_rows: usize, cols: usize) {
    let lines = [
        "zellij-linear — keybinds",
        "",
        "  j / ↓   next issue",
        "  k / ↑   previous issue",
        "  g       jump to top",
        "  G       jump to bottom",
        "  r       refresh now",
        "  c       send to Claude (paste)",
        "  C       send to Claude + submit",
        "  y       copy issue body",
        "  Y       copy formatted prompt",
        "  o       open in browser",
        "  ?       toggle this help",
        "  Esc     back / hide plugin",
    ];
    for line in lines {
        println!("{}", truncate(line, cols));
    }
}
