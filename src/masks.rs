use libvips::{ops, VipsImage, error::Error};

pub fn create_svg_mask(width: i32, height: i32, radius: f64) -> Result<VipsImage, Error> {
    let svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\">
            <rect rx=\"{r}\" ry=\"{r}\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\" fill=\"#fff\"/>
        </svg>",
        w = width,
        h = height,
        r = radius
    );

    let mask = VipsImage::new_from_buffer(svg.as_bytes(), "")?;
    let mask_alpha = ops::extract_band(&mask, 3)?;
    Ok(mask_alpha)
}

pub fn create_svg_border(
    width: i32,
    height: i32,
    radius: f64,
) -> Result<VipsImage, Error> {
    let stroke_width = 2.0;
    let half_sw = 1.0;

    let w = width as f64 - stroke_width;
    let h = height as f64 - stroke_width;

    let mut r = radius - half_sw;
    if r < 0.0 { r = 0.0; }

    let svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\">
            <rect rx=\"{r}\" ry=\"{r}\"
                  x=\"{half_sw}\" y=\"{half_sw}\"
                  width=\"{w}\" height=\"{h}\"
                  fill=\"none\"
                  stroke=\"rgb(242, 243, 243)\" stroke-opacity=\"0.3\" stroke-width=\"{stroke_width}\"/>
        </svg>",
        width = width,
        height = height,
        r = r,
        half_sw = half_sw,
        w = w,
        h = h,
        stroke_width = stroke_width
    );

    VipsImage::new_from_buffer(svg.as_bytes(), "")
}


pub fn create_svg_gradient(width: i32, height: i32, gradient: f64) -> Result<VipsImage, Error> {
    let grad_height = (height as f64 * gradient).clamp(0.0, height as f64);
    let start_y = height as f64 - grad_height;

    let svg = format!(
        "<svg viewBox=\"0 0 {width} {height}\">
            <defs>
                <linearGradient id=\"bottom_grad\" x1=\"0%\" y1=\"0%\" x2=\"0%\" y2=\"100%\">
                    <stop offset=\"0%\" stop-color=\"rgb(10, 21, 31)\" stop-opacity=\"0\" />
                    <stop offset=\"100%\" stop-color=\"rgb(10, 21, 31)\" stop-opacity=\"0.95\" />
                </linearGradient>
            </defs>
            <rect x=\"0\" y=\"{start_y}\" width=\"{width}\" height=\"{grad_height}\" fill=\"url(#bottom_grad)\" />
        </svg>",
        width = width,
        height = height,
        start_y = start_y,
        grad_height = grad_height
    );

    VipsImage::new_from_buffer(svg.as_bytes(), "")
}
