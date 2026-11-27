//! What a dialog asks the player for (vanilla `server/dialog/input`): each
//! input's value goes to the buttons' actions under its key.

use crate::component::Component;

/// `Input`: a control and the key its value goes by.
#[derive(Clone, Debug)]
pub struct Input {
    pub key: String,
    pub control: InputControl,
}

/// `InputControl`'s kinds (`InputControlTypes`).
#[derive(Clone, Debug)]
pub enum InputControl {
    Text(TextInput),
    Boolean(BooleanInput),
    SingleOption(SingleOptionInput),
    NumberRange(NumberRangeInput),
}

/// `TextInput`.
#[derive(Clone, Debug)]
pub struct TextInput {
    pub width: u32,
    pub label: Component,
    pub label_visible: bool,
    pub initial: String,
    pub max_length: u32,
    pub multiline: Option<Multiline>,
}

/// `TextInput.MultilineOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Multiline {
    pub max_lines: Option<u32>,
    /// 1 to 512.
    pub height: Option<u32>,
}

impl Multiline {
    /// `TextInputHandler`: the box's height, the given one or enough for
    /// `max_lines` (4 without), at most 512. Java's `int` wraps for huge
    /// counts; the negative heights that gives are 0 here.
    pub fn box_height(self) -> u32 {
        self.height.unwrap_or_else(|| {
            let lines = self.max_lines.unwrap_or(4) as i32;
            9i32.wrapping_mul(lines).wrapping_add(8).clamp(0, 512) as u32
        })
    }
}

/// `BooleanInput`: a checkbox.
#[derive(Clone, Debug)]
pub struct BooleanInput {
    pub label: Component,
    pub initial: bool,
    pub on_true: String,
    pub on_false: String,
}

/// `SingleOptionInput`: a button that cycles through options.
#[derive(Clone, Debug)]
pub struct SingleOptionInput {
    pub width: u32,
    pub entries: Vec<OptionEntry>,
    pub label: Component,
    pub label_visible: bool,
}

impl SingleOptionInput {
    /// The entry marked initial, else the first.
    pub fn initial(&self) -> usize {
        self.entries.iter().position(|e| e.initial).unwrap_or(0)
    }
}

/// `SingleOptionInput.Entry`.
#[derive(Clone, Debug)]
pub struct OptionEntry {
    pub id: String,
    pub display: Option<Component>,
    pub initial: bool,
}

impl OptionEntry {
    /// `displayOrDefault`: the display text, else the id.
    pub fn display(&self) -> Component {
        self.display
            .clone()
            .unwrap_or_else(|| Component::text(self.id.as_str()))
    }
}

/// `NumberRangeInput`: a slider.
#[derive(Clone, Debug)]
pub struct NumberRangeInput {
    pub width: u32,
    pub label: Component,
    /// A translation key taking the label and the value.
    pub label_format: String,
    pub range: RangeInfo,
}

impl NumberRangeInput {
    /// `computeLabel`.
    pub fn label_for(&self, value: &str) -> Component {
        Component::translatable(
            self.label_format.as_str(),
            vec![self.label.clone(), Component::text(value)],
        )
    }
}

/// `NumberRangeInput.RangeInfo`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RangeInfo {
    pub start: f32,
    pub end: f32,
    pub initial: Option<f32>,
    /// Positive.
    pub step: Option<f32>,
}

impl RangeInfo {
    /// `computeScaledValue`: the slider's `0..=1` as a value, on a step
    /// counted from the initial value and kept inside the range.
    pub fn scaled(&self, slider: f32) -> f32 {
        let in_range = self.start + slider * (self.end - self.start);
        let Some(step) = self.step else {
            return in_range;
        };
        let initial = self.initial_value();
        let steps = java_round((in_range - initial) / step);
        let result = initial + steps as f32 * step;
        if !self.out_of_range(result) {
            return result;
        }
        let one_less = steps - steps.signum();
        initial + one_less as f32 * step
    }

    fn out_of_range(&self, value: f32) -> bool {
        let slider = self.slider_position(value);
        !(0.0..=1.0).contains(&slider)
    }

    /// `initialScaledValue`: the given one, else the middle.
    fn initial_value(&self) -> f32 {
        self.initial.unwrap_or((self.start + self.end) / 2.0)
    }

    /// `initialSliderValue`.
    pub fn initial_slider(&self) -> f32 {
        self.slider_position(self.initial_value())
    }

    /// `scaledValueToSlider`.
    fn slider_position(&self, value: f32) -> f32 {
        if self.start == self.end {
            0.5
        } else {
            (value - self.start) / (self.end - self.start)
        }
    }
}

/// `Math.round(float)`.
fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

/// `NumberRangeHandler.valueToString`: whole values without a fraction,
/// others as `Float.toString` writes them.
pub fn value_to_string(value: f32) -> String {
    if value.is_finite() && value.trunc() == value && value.abs() < 2_147_483_648.0 {
        return (value as i32).to_string();
    }
    java_float_string(value)
}

/// `Float.toString`: plain between 10⁻³ and 10⁷, scientific outside.
fn java_float_string(value: f32) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let magnitude = value.abs();
    if magnitude == 0.0 || (1e-3..1e7).contains(&magnitude) {
        let text = value.to_string();
        return if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        };
    }
    let text = format!("{value:E}");
    match text.split_once('E') {
        Some((mantissa, exponent)) if !mantissa.contains('.') => {
            format!("{mantissa}.0E{exponent}")
        }
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_count_from_the_initial_value_and_stay_in_range() {
        let range = RangeInfo {
            start: 0.0,
            end: 10.0,
            initial: Some(1.0),
            step: Some(3.0),
        };
        assert_eq!(range.scaled(0.0), 1.0, "0 is 1 step under, out of range");
        assert_eq!(range.scaled(0.4), 4.0);
        assert_eq!(range.scaled(1.0), 10.0);
        assert!((range.initial_slider() - 0.1).abs() < 1e-6);
        let free = RangeInfo {
            step: None,
            initial: None,
            ..range
        };
        assert_eq!(free.scaled(0.25), 2.5);
        assert_eq!(free.initial_slider(), 0.5);
    }

    #[test]
    fn values_print_as_java_does() {
        assert_eq!(value_to_string(4.0), "4");
        assert_eq!(value_to_string(-2.0), "-2");
        assert_eq!(value_to_string(2.5), "2.5");
        assert_eq!(value_to_string(0.0001), "1.0E-4");
        assert_eq!(value_to_string(1.5e8), "150000000");
        assert_eq!(value_to_string(3.5e9), "3.5E9");
    }

    #[test]
    fn multiline_boxes_fit_their_lines() {
        let lines = |max_lines, height| Multiline { max_lines, height }.box_height();
        assert_eq!(lines(None, None), 44);
        assert_eq!(lines(Some(2), None), 26);
        assert_eq!(lines(Some(100), None), 512);
        assert_eq!(lines(Some(2), Some(100)), 100);
        assert_eq!(lines(Some(477_218_589), None), 13, "wraps like Java");
    }
}
