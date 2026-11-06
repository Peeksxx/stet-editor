use ratatui::{Frame, layout::Rect, style::Color};

const ALPHA: &[u8; 96 * 32] = include_bytes!("../assets/stet.alpha");

pub fn draw(frame: &mut Frame, area: Rect, background: Color) {
    let width = area.width.saturating_sub(8).min(48) / 6 * 6;
    let height = width / 6;
    if width < 30 || area.height < height + 4 {
        return;
    }
    let x = area.x + (area.width - width) / 2;
    let y = area.y + (area.height - height) / 2;
    let sample = |column: u16, row: u16| {
        let left = column as usize * 96 / width as usize;
        let right = (column as usize + 1) * 96 / width as usize;
        let top = row as usize * 32 / (height as usize * 2);
        let bottom = (row as usize + 1) * 32 / (height as usize * 2);
        let mut sum = 0u32;
        for row in top..bottom {
            for column in left..right {
                sum += ALPHA[row * 96 + column] as u32;
            }
        }
        (sum / ((right - left) * (bottom - top)) as u32) as u8
    };
    for row in 0..height {
        for column in 0..width {
            let upper = sample(column, row * 2);
            let lower = sample(column, row * 2 + 1);
            if upper == 0 && lower == 0 {
                continue;
            }
            let cell = &mut frame.buffer_mut()[(x + column, y + row)];
            if matches!(background, Color::Rgb(..)) {
                cell.set_symbol("▀")
                    .set_fg(blend(upper, background))
                    .set_bg(blend(lower, background));
            } else {
                let symbol = match (upper >= 128, lower >= 128) {
                    (true, true) => "█",
                    (true, false) => "▀",
                    (false, true) => "▄",
                    (false, false) => continue,
                };
                cell.set_symbol(symbol)
                    .set_fg(Color::Rgb(0, 113, 188))
                    .set_bg(background);
            }
        }
    }
}

fn blend(alpha: u8, background: Color) -> Color {
    if let Color::Rgb(r, g, b) = background {
        let mix = |ink: u8, paper: u8| {
            ((ink as u32 * alpha as u32 + paper as u32 * (255 - alpha as u32)) / 255) as u8
        };
        Color::Rgb(mix(0, r), mix(113, g), mix(188, b))
    } else {
        background
    }
}
