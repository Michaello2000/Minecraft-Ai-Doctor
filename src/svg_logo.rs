//! Figma's angular gradient uses foreignObject/CSS. Keep the supplied SVG as source,
//! and compile its exact color stops into portable SVG wedges for native renderers.
pub fn portable(source: &str) -> Result<String, String> {
    if !source.contains("foreignObject") {
        return Ok(source.into());
    }
    let marker = "data-figma-gradient-fill=\"";
    let start = source
        .find(marker)
        .ok_or("SVG angular gradient metadata missing")?
        + marker.len();
    let end = source[start..]
        .find('"')
        .ok_or("SVG gradient metadata is incomplete")?
        + start;
    let value: serde_json::Value = serde_json::from_str(
        &source[start..end]
            .replace("&#34;", "\"")
            .replace("&quot;", "\""),
    )
    .map_err(|e| e.to_string())?;
    let stops = value["stops"]
        .as_array()
        .ok_or("SVG gradient stops missing")?
        .iter()
        .map(|s| {
            let p = s["position"].as_f64().ok_or("Invalid gradient stop")?;
            let c = &s["color"];
            Ok((
                p,
                [
                    c["r"].as_f64().ok_or("Invalid red")?,
                    c["g"].as_f64().ok_or("Invalid green")?,
                    c["b"].as_f64().ok_or("Invalid blue")?,
                ],
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    if stops.len() < 2 {
        return Err("Not enough gradient stops".into());
    }
    let mut svg = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"500\" height=\"500\" viewBox=\"0 0 500 500\">",
    );
    for i in 0..720 {
        let t = f64::from(i) / 720.0;
        let pair = stops
            .windows(2)
            .find(|p| t >= p[0].0 && t <= p[1].0)
            .unwrap_or(&stops[stops.len() - 2..]);
        let f = ((t - pair[0].0) / (pair[1].0 - pair[0].0)).clamp(0.0, 1.0);
        let color = (0..3)
            .map(|j| {
                ((pair[0].1[j] + (pair[1].1[j] - pair[0].1[j]) * f) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8
            })
            .collect::<Vec<_>>();
        let a = t * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
        let b = a + std::f64::consts::TAU / 720.0 + 0.0002;
        let p = |r: f64, angle: f64| (250.0 + r * angle.cos(), 250.0 + r * angle.sin());
        let (x1, y1) = p(250.0, a);
        let (x2, y2) = p(250.0, b);
        let (x3, y3) = p(205.0, b);
        let (x4, y4) = p(205.0, a);
        svg.push_str(&format!("<path fill=\"#{:02x}{:02x}{:02x}\" d=\"M{x1:.4} {y1:.4}A250 250 0 0 1 {x2:.4} {y2:.4}L{x3:.4} {y3:.4}A205 205 0 0 0 {x4:.4} {y4:.4}Z\"/>",color[0],color[1],color[2]));
    }
    svg.push_str("</svg>");
    Ok(svg)
}

pub fn render(svg: &str, size: u32) -> Result<resvg::tiny_skia::Pixmap, String> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|e| e.to_string())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or("Invalid logo size")?;
    let scale = size as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
