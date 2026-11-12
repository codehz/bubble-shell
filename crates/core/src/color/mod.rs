//! CSS Color 4 values, carried as extended nonlinear sRGB.
//!
//! Authors may write any CSS color syntax except `currentColor`, relative
//! colors, `color-mix()`, and system colors. Everything resolves to
//! extended sRGB so hosts can render wide-gamut values on capable displays;
//! hosts that only paint sRGB clamp on their side. Numeric colors are still
//! `0xAARRGGBB` (alpha in the high byte), matching the wire format.
//!
//! Supported: named colors, `transparent`, `#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`,
//! `rgb()`/`rgba()`, `hsl()`/`hsla()`, `hwb()`, `lab()`, `lch()`, `oklab()`,
//! `oklch()`, and `color()` with `srgb`, `srgb-linear`, `display-p3`,
//! `a98-rgb`, `prophoto-rgb`, `rec2020`, `xyz`, `xyz-d50`, or `xyz-d65`.
//! Both comma and space/slash component syntax are accepted; components may
//! be `none`.

mod names;

use std::f64::consts::PI;

/// One color in extended nonlinear sRGB. Components may fall outside
/// `[0, 1]` for wide-gamut values; alpha is always clamped.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::new(0.0, 0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::new(1.0, 1.0, 1.0, 1.0);

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// `0xAARRGGBB`; the only numeric color form.
    pub fn from_argb(argb: u32) -> Self {
        Self {
            r: ((argb >> 16) & 0xff) as f32 / 255.0,
            g: ((argb >> 8) & 0xff) as f32 / 255.0,
            b: (argb & 0xff) as f32 / 255.0,
            a: ((argb >> 24) & 0xff) as f32 / 255.0,
        }
    }

    /// Clamps to sRGB and packs as `0xAARRGGBB`.
    pub fn to_argb(self) -> u32 {
        let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u32;
        (channel(self.a) << 24) | (channel(self.r) << 16) | (channel(self.g) << 8) | channel(self.b)
    }

    pub fn is_visible(self) -> bool {
        self.a > 0.0
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::TRANSPARENT
    }
}

/// Parses one CSS color value.
pub fn parse(input: &str) -> Result<Color, String> {
    let text = input.trim();
    if text.is_empty() {
        return Err("color is empty".into());
    }
    let lower = text.to_ascii_lowercase();
    if lower == "transparent" {
        return Ok(Color::TRANSPARENT);
    }
    if let Some(digits) = lower.strip_prefix('#') {
        return parse_hex(digits);
    }
    let Some(open) = lower.find('(') else {
        return names::lookup(&lower)
            .map(|[r, g, b]| Color::new(r, g, b, 1.0))
            .ok_or_else(|| format!("unknown color keyword `{text}`"));
    };
    let name = lower[..open].trim();
    let body = lower[open + 1..]
        .strip_suffix(')')
        .ok_or("color function is missing `)`")?;
    match name {
        "rgb" | "rgba" => parse_rgb(body),
        "hsl" | "hsla" => parse_hsl(body),
        "hwb" => parse_hwb(body),
        "lab" => parse_lab(body),
        "lch" => parse_lch(body),
        "oklab" => parse_oklab(body),
        "oklch" => parse_oklch(body),
        "color" => parse_color_function(body),
        other => Err(format!("unsupported color function `{other}()`")),
    }
}

fn parse_hex(digits: &str) -> Result<Color, String> {
    let value = |index: usize| -> Result<u32, String> {
        digits
            .as_bytes()
            .get(index)
            .and_then(|byte| (*byte as char).to_digit(16))
            .ok_or_else(|| format!("invalid hex color `#{digits}`"))
    };
    let (r, g, b, a) = match digits.len() {
        3 | 4 => {
            let r = value(0)? * 17;
            let g = value(1)? * 17;
            let b = value(2)? * 17;
            let a = if digits.len() == 4 {
                value(3)? * 17
            } else {
                255
            };
            (r, g, b, a)
        }
        6 | 8 => {
            let mut channels = [255u32; 4];
            for (index, channel) in channels.iter_mut().take(digits.len() / 2).enumerate() {
                *channel = value(index * 2)? * 16 + value(index * 2 + 1)?;
            }
            (channels[0], channels[1], channels[2], channels[3])
        }
        other => return Err(format!("unsupported hex color length {other}")),
    };
    Ok(Color::from_argb((a << 24) | (r << 16) | (g << 8) | b))
}

/// Splits function arguments into components plus an optional `/ alpha`.
/// Legacy comma syntax keeps alpha as the fourth component.
fn arguments(body: &str) -> Result<(Vec<&str>, Option<&str>), String> {
    if body.contains(',') {
        if body.contains('/') {
            return Err("`/` cannot be mixed with comma syntax".into());
        }
        return Ok((body.split(',').map(str::trim).collect(), None));
    }
    let mut components = Vec::new();
    let mut alpha = None;
    let mut in_alpha = false;
    for token in body.split_whitespace() {
        if let Some(rest) = token.strip_prefix('/') {
            if in_alpha {
                return Err("duplicate color alpha".into());
            }
            in_alpha = true;
            if !rest.is_empty() {
                alpha = Some(rest);
            }
            continue;
        }
        if in_alpha {
            if alpha.is_some() {
                return Err("duplicate color alpha".into());
            }
            alpha = Some(token);
        } else {
            components.push(token);
        }
    }
    Ok((components, alpha))
}

fn resolve_alpha(components: &mut Vec<&str>, alpha: Option<&str>) -> Result<f64, String> {
    let token = match (alpha, components.len()) {
        (Some(token), 3) => Some(token),
        (None, 3) => None,
        (None, 4) => Some(components.pop().unwrap()),
        _ => return Err("expected 3 color components and optional alpha".into()),
    };
    match token {
        None => Ok(1.0),
        Some(value) => number_or_percent(value).map(|(value, _)| value.clamp(0.0, 1.0)),
    }
}

fn number(token: &str) -> Result<f64, String> {
    if token.eq_ignore_ascii_case("none") {
        return Ok(0.0);
    }
    token
        .parse::<f64>()
        .map_err(|_| format!("`{token}` is not a number"))
        .and_then(|value| {
            if value.is_finite() {
                Ok(value)
            } else {
                Err(format!("`{token}` is not a finite number"))
            }
        })
}

/// Returns the value and whether the token was a percentage (already divided
/// by 100).
fn number_or_percent(token: &str) -> Result<(f64, bool), String> {
    if let Some(percent) = token.strip_suffix('%') {
        Ok((number(percent.trim())? / 100.0, true))
    } else {
        Ok((number(token)?, false))
    }
}

fn angle(token: &str) -> Result<f64, String> {
    if token.eq_ignore_ascii_case("none") {
        return Ok(0.0);
    }
    for (suffix, scale) in [
        ("turn", 360.0),
        ("grad", 0.9),
        ("deg", 1.0),
        ("rad", 180.0 / PI),
    ] {
        if let Some(value) = token.strip_suffix(suffix) {
            return Ok(number(value.trim())? * scale);
        }
    }
    number(token)
}

fn alpha_of(components: &mut Vec<&str>, alpha: Option<&str>) -> Result<f64, String> {
    resolve_alpha(components, alpha)
}

fn rgb_channel(token: &str) -> Result<f64, String> {
    let (value, percent) = number_or_percent(token)?;
    let value = if percent { value * 255.0 } else { value };
    Ok(value.clamp(0.0, 255.0) / 255.0)
}

fn parse_rgb(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("rgb() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let r = rgb_channel(components[0])?;
    let g = rgb_channel(components[1])?;
    let b = rgb_channel(components[2])?;
    Ok(Color::new(r as f32, g as f32, b as f32, a as f32))
}

fn parse_hsl(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("hsl() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let hue = angle(components[0])?;
    let saturation = number_or_percent(components[1])?.0.clamp(0.0, 1.0);
    let lightness = number_or_percent(components[2])?.0.clamp(0.0, 1.0);
    let [r, g, b] = hsl_to_rgb(hue, saturation, lightness);
    Ok(Color::new(r as f32, g as f32, b as f32, a as f32))
}

fn parse_hwb(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("hwb() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let hue = angle(components[0])?;
    let white = number_or_percent(components[1])?.0.max(0.0);
    let black = number_or_percent(components[2])?.0.max(0.0);
    let [r, g, b] = hwb_to_rgb(hue, white, black);
    Ok(Color::new(r as f32, g as f32, b as f32, a as f32))
}

fn parse_lab(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("lab() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let lightness = lightness_component(components[0], 100.0)?;
    let a_axis = axis_component(components[1], 125.0)?;
    let b_axis = axis_component(components[2], 125.0)?;
    let linear = xyz_d50_to_linear_srgb(lab_to_xyz_d50(lightness, a_axis, b_axis));
    Ok(linear_to_color(linear, a))
}

fn parse_lch(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("lch() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let lightness = lightness_component(components[0], 100.0)?;
    let chroma = axis_component(components[1], 150.0)?;
    let hue = angle(components[2])?;
    let lab = lch_to_lab(lightness, chroma, hue);
    let linear = xyz_d50_to_linear_srgb(lab_to_xyz_d50(lab[0], lab[1], lab[2]));
    Ok(linear_to_color(linear, a))
}

fn parse_oklab(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("oklab() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let lightness = lightness_component(components[0], 1.0)?;
    let a_axis = axis_component(components[1], 0.4)?;
    let b_axis = axis_component(components[2], 0.4)?;
    Ok(linear_to_color(
        oklab_to_linear_srgb(lightness, a_axis, b_axis),
        a,
    ))
}

fn parse_oklch(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() < 3 || components.len() > 4 {
        return Err("oklch() expects 3 components and optional alpha".into());
    }
    let a = alpha_of(&mut components, alpha)?;
    let lightness = lightness_component(components[0], 1.0)?;
    let chroma = axis_component(components[1], 0.4)?;
    let hue = angle(components[2])?;
    let lab = lch_to_lab(lightness, chroma, hue);
    Ok(linear_to_color(
        oklab_to_linear_srgb(lab[0], lab[1], lab[2]),
        a,
    ))
}

/// `L` accepts `0..max` either as a number or as a percentage of `max`.
fn lightness_component(token: &str, max: f64) -> Result<f64, String> {
    let (value, percent) = number_or_percent(token)?;
    Ok(if percent { value * max } else { value })
}

/// Chroma and Lab a/b axes: a percentage maps to the CSS reference range.
fn axis_component(token: &str, max: f64) -> Result<f64, String> {
    let (value, percent) = number_or_percent(token)?;
    Ok(if percent { value * max } else { value })
}

fn parse_color_function(body: &str) -> Result<Color, String> {
    let (mut components, alpha) = arguments(body)?;
    if components.len() != 4 {
        return Err("color() expects a color space and 3 components".into());
    }
    let space = components.remove(0);
    let a = alpha_of(&mut components, alpha)?;
    let value = |token: &str| number_or_percent(token).map(|(value, _)| value);
    let c0 = value(components[0])?;
    let c1 = value(components[1])?;
    let c2 = value(components[2])?;
    let linear = match space {
        "srgb" => [srgb_to_linear(c0), srgb_to_linear(c1), srgb_to_linear(c2)],
        "srgb-linear" => [c0, c1, c2],
        "display-p3" => p3_to_linear_srgb([c0, c1, c2]),
        "a98-rgb" => a98_to_linear_srgb([c0, c1, c2]),
        "prophoto-rgb" => prophoto_to_linear_srgb([c0, c1, c2]),
        "rec2020" => rec2020_to_linear_srgb([c0, c1, c2]),
        "xyz" | "xyz-d65" => xyz_d65_to_linear_srgb([c0, c1, c2]),
        "xyz-d50" => xyz_d50_to_linear_srgb([c0, c1, c2]),
        other => return Err(format!("unsupported color() space `{other}`")),
    };
    Ok(linear_to_color(linear, a))
}

fn linear_to_color(linear: [f64; 3], alpha: f64) -> Color {
    Color::new(
        linear_to_srgb(linear[0]) as f32,
        linear_to_srgb(linear[1]) as f32,
        linear_to_srgb(linear[2]) as f32,
        alpha.clamp(0.0, 1.0) as f32,
    )
}

fn hsl_to_rgb(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let hue = hue.rem_euclid(360.0);
    let a = saturation * lightness.min(1.0 - lightness);
    let channel = |n: f64| {
        let k = (n + hue / 30.0).rem_euclid(12.0);
        lightness - a * (-1.0f64).max((k - 3.0).min(9.0 - k).min(1.0))
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}

fn hwb_to_rgb(hue: f64, white: f64, black: f64) -> [f64; 3] {
    if white + black >= 1.0 {
        let gray = white / (white + black);
        return [gray, gray, gray];
    }
    let rgb = hsl_to_rgb(hue, 1.0, 0.5);
    let scale = 1.0 - white - black;
    [
        rgb[0] * scale + white,
        rgb[1] * scale + white,
        rgb[2] * scale + white,
    ]
}

fn lch_to_lab(lightness: f64, chroma: f64, hue: f64) -> [f64; 3] {
    let radians = hue.to_radians();
    [lightness, chroma * radians.cos(), chroma * radians.sin()]
}

fn lab_to_xyz_d50(lightness: f64, a: f64, b: f64) -> [f64; 3] {
    let kappa = 24389.0 / 27.0;
    let epsilon = 216.0 / 24389.0;
    let f1 = (lightness + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let f = |value: f64| {
        let cube = value * value * value;
        if cube > epsilon {
            cube
        } else {
            (116.0 * value - 16.0) / kappa
        }
    };
    let x = f(f0);
    let y = if lightness > kappa * epsilon {
        let value = (lightness + 16.0) / 116.0;
        value * value * value
    } else {
        lightness / kappa
    };
    let z = f(f2);
    [x * 0.9642956764295677, y, z * 0.8251046025104602]
}

fn oklab_to_linear_srgb(lightness: f64, a: f64, b: f64) -> [f64; 3] {
    let l = lightness + 0.3963377774 * a + 0.2158037573 * b;
    let m = lightness - 0.1055613458 * a - 0.0638541728 * b;
    let s = lightness - 0.0894841775 * a - 1.2914855480 * b;
    let l = l * l * l;
    let m = m * m * m;
    let s = s * s * s;
    [
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    ]
}

fn srgb_to_linear(value: f64) -> f64 {
    let sign = value.signum();
    let value = value.abs();
    if value <= 0.04045 {
        value / 12.92 * sign
    } else {
        sign * ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f64) -> f64 {
    let sign = value.signum();
    let value = value.abs();
    if value > 0.0031308 {
        sign * (1.055 * value.powf(1.0 / 2.4) - 0.055)
    } else {
        12.92 * value * sign
    }
}

fn xyz_d50_to_xyz_d65([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        0.9554734527042182 * x - 0.023098536874261423 * y + 0.0632593086610217 * z,
        -0.028369706963208136 * x + 1.0099954580058226 * y + 0.021041398966943008 * z,
        0.012314001688319899 * x - 0.020507696433477912 * y + 1.3303659366080753 * z,
    ]
}

fn xyz_d65_to_linear_srgb([x, y, z]: [f64; 3]) -> [f64; 3] {
    [
        3.2409699419045226 * x - 1.537383177570094 * y - 0.4986107602930034 * z,
        -0.9692436362808796 * x + 1.8759675015077202 * y + 0.04155505740717559 * z,
        0.05563007969699366 * x - 0.20397695888897652 * y + 1.0569715142428786 * z,
    ]
}

fn xyz_d50_to_linear_srgb(xyz: [f64; 3]) -> [f64; 3] {
    xyz_d65_to_linear_srgb(xyz_d50_to_xyz_d65(xyz))
}

fn p3_to_linear_srgb([r, g, b]: [f64; 3]) -> [f64; 3] {
    let [r, g, b] = [srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)];
    let xyz = [
        0.4865709486482162 * r + 0.26566769316909306 * g + 0.1982172852343625 * b,
        0.2289745640697488 * r + 0.6917385218365064 * g + 0.079286914093745 * b,
        0.04511338185890264 * g + 1.043944368900976 * b,
    ];
    xyz_d65_to_linear_srgb(xyz)
}

fn a98_to_linear_srgb([r, g, b]: [f64; 3]) -> [f64; 3] {
    let gamma = 563.0 / 256.0;
    let decode = |value: f64| value.signum() * value.abs().powf(gamma);
    let [r, g, b] = [decode(r), decode(g), decode(b)];
    let xyz = [
        0.5766690429101305 * r + 0.1855582379065463 * g + 0.1882286462349947 * b,
        0.29734497525053605 * r + 0.6273635662554661 * g + 0.07529145849399788 * b,
        0.02703136138641234 * r + 0.07068885253582723 * g + 0.9913375368376388 * b,
    ];
    xyz_d65_to_linear_srgb(xyz)
}

fn prophoto_to_linear_srgb([r, g, b]: [f64; 3]) -> [f64; 3] {
    let decode = |value: f64| {
        if value.abs() <= 16.0 / 512.0 {
            value / 16.0
        } else {
            value.signum() * value.abs().powf(1.8)
        }
    };
    let [r, g, b] = [decode(r), decode(g), decode(b)];
    let xyz_d50 = [
        0.7977604896723027 * r + 0.13518583717574031 * g + 0.0313493495815248 * b,
        0.2880711282292934 * r + 0.7118432178101014 * g + 0.00008565396060525902 * b,
        0.8251046025104601 * b,
    ];
    xyz_d50_to_linear_srgb(xyz_d50)
}

fn rec2020_to_linear_srgb([r, g, b]: [f64; 3]) -> [f64; 3] {
    let alpha = 1.09929682680944;
    let beta = 0.018053968510807;
    let decode = |value: f64| {
        let sign = value.signum();
        let value = value.abs();
        if value < beta * 4.5 {
            value / 4.5 * sign
        } else {
            sign * ((value + alpha - 1.0) / alpha).powf(1.0 / 0.45)
        }
    };
    let [r, g, b] = [decode(r), decode(g), decode(b)];
    let xyz = [
        0.6369580483012914 * r + 0.14461690358620832 * g + 0.1688809751641721 * b,
        0.2627002120112671 * r + 0.6779980715188708 * g + 0.05930171646986196 * b,
        0.028072693049087428 * g + 1.060985057710791 * b,
    ];
    xyz_d65_to_linear_srgb(xyz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: Color, expected: [f32; 4]) {
        let values = [actual.r, actual.g, actual.b, actual.a];
        for (index, (actual, expected)) in values.iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() < 0.0005,
                "channel {index}: {actual} != {expected} (color {actual_color:?})",
                actual_color = actual
            );
        }
    }

    #[test]
    fn hex_forms() {
        assert_close(parse("#f00").unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert_close(parse("#FF000080").unwrap(), [1.0, 0.0, 0.0, 128.0 / 255.0]);
        assert_close(parse("#0000").unwrap(), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(parse("#12").unwrap_err(), "unsupported hex color length 2");
        assert!(parse("#gg0000").is_err());
        assert!(parse("#é").is_err());
    }

    #[test]
    fn keywords_and_transparent() {
        assert_close(
            parse("RebeccaPurple").unwrap(),
            [
                0x66 as f32 / 255.0,
                0x33 as f32 / 255.0,
                0x99 as f32 / 255.0,
                1.0,
            ],
        );
        assert_close(parse("transparent").unwrap(), [0.0, 0.0, 0.0, 0.0]);
        assert!(parse("notacolor").is_err());
    }

    #[test]
    fn rgb_syntaxes() {
        assert_close(parse("rgb(255, 0, 0)").unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert_close(parse("rgba(0, 0, 255, 0.5)").unwrap(), [0.0, 0.0, 1.0, 0.5]);
        assert_close(
            parse("rgb(100% 0% 0% / 25%)").unwrap(),
            [1.0, 0.0, 0.0, 0.25],
        );
        assert_close(parse("rgb(300 -20 0)").unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert!(parse("rgb(1, 2)").is_err());
    }

    #[test]
    fn hsl_and_hwb() {
        assert_close(parse("hsl(120, 100%, 50%)").unwrap(), [0.0, 1.0, 0.0, 1.0]);
        assert_close(
            parse("hsl(0.5turn 100% 50% / .5)").unwrap(),
            [0.0, 1.0, 1.0, 0.5],
        );
        assert_close(parse("hwb(0 0% 0%)").unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert_close(parse("hwb(0 20% 20%)").unwrap(), [0.8, 0.2, 0.2, 1.0]);
    }

    #[test]
    fn lab_and_oklab_match_primaries() {
        assert_close(parse("lab(100% 0 0)").unwrap(), [1.0, 1.0, 1.0, 1.0]);
        assert_close(parse("lab(0% 0 0)").unwrap(), [0.0, 0.0, 0.0, 1.0]);
        assert_close(parse("oklab(100% 0 0)").unwrap(), [1.0, 1.0, 1.0, 1.0]);
        assert_close(
            parse("oklch(0.627955 0.257683 29.2339)").unwrap(),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert_close(parse("lch(100% 0 0)").unwrap(), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn color_function_spaces() {
        assert_close(parse("color(srgb 1 0 0)").unwrap(), [1.0, 0.0, 0.0, 1.0]);
        assert_close(
            parse("color(srgb-linear 1 0 0)").unwrap(),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert_close(
            parse("color(display-p3 1 0 0)").unwrap(),
            [1.0930, -0.2267, -0.1501, 1.0],
        );
        assert_close(
            parse("color(rec2020 1 0 0)").unwrap(),
            [1.2482, -0.3879, -0.1435, 1.0],
        );
        assert_close(
            parse("color(xyz-d65 0.41239079926595934 0.21263900587151027 0.01933081871559182)")
                .unwrap(),
            [1.0, 0.0, 0.0, 1.0],
        );
        assert!(parse("color(cmyk 1 0 0)").is_err());
    }

    #[test]
    fn argb_roundtrip() {
        assert_eq!(Color::from_argb(0xff123456).to_argb(), 0xff123456);
        assert_eq!(Color::from_argb(0x00000000).to_argb(), 0);
    }
}
