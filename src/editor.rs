//! Presentation bindings reconstructed from inert packaged UI metadata.
use crate::{
    document::ParamSet,
    schema::{Choice, Parameter, Schema},
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Deserialize)]
pub struct Control {
    pub id: String,
    pub block: String,
    pub offset: usize,
    pub label: String,
    pub choices: Vec<Choice>,
    pub display: String,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub bias: i32,
}
#[derive(Clone, Debug)]
pub struct Edit {
    pub block: String,
    pub index: usize,
    pub bytes: Vec<u8>,
}
impl Control {
    pub fn remap(&self, from: &str, to: &str) -> Self {
        let mut c = self.clone();
        c.block = c.block.replace(from, to);
        c
    }
    pub fn parameter<'a>(&self, s: &'a Schema) -> Result<(usize, &'a Parameter)> {
        s.block(&self.block)?
            .parameters
            .iter()
            .enumerate()
            .find(|(_, p)| p.offset == self.offset)
            .context("control binding missing")
    }
    pub fn value(&self, s: &Schema, set: &ParamSet) -> Result<i32> {
        let (_, p) = self.parameter(s)?;
        let d = set.get(&self.block).context("parameter block not loaded")?;
        Ok(p.decode_number(d)? + p.bias - self.bias)
    }
    pub fn range(&self, s: &Schema) -> Result<(i32, i32)> {
        let (_, p) = self.parameter(s)?;
        Ok((
            self.min.unwrap_or(p.min + p.bias - self.bias),
            self.max.unwrap_or(p.max + p.bias - self.bias),
        ))
    }
    pub fn set(&self, s: &Schema, set: &mut ParamSet, value: i32) -> Result<Edit> {
        let (i, p) = self.parameter(s)?;
        let b = s.block(&self.block)?;
        ensure!(!b.readonly, "read-only field");
        let (min, max) = self.range(s)?;
        ensure!((min..=max).contains(&value), "control out of range");
        let bytes = p.encode_number(value + self.bias - p.bias)?;
        let data = set
            .get_mut(&self.block)
            .context("parameter block not loaded")?;
        ensure!(data.len() >= p.offset + p.width, "short block");
        data[p.offset..p.offset + p.width].copy_from_slice(&bytes);
        Ok(Edit {
            block: self.block.clone(),
            index: i,
            bytes,
        })
    }
    pub fn parse_text(&self, text: &str) -> Option<f64> {
        if self.display == "off_negative" && text.trim().eq_ignore_ascii_case("OFF") {
            return Some(-1.);
        }
        let token = text
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_digit() || ['+', '-', '.'].contains(c))
            .collect::<String>();
        let value = token.parse::<f64>().ok()?;
        Some(match self.display.as_str() {
            "half_db" => value * 2.,
            "tenths_s" => value * 10.,
            "double_percent" => value / 2.,
            "plus_one" => value - 1.,
            _ => value,
        })
    }
    pub fn text(&self, value: i32) -> String {
        if let Some(c) = self.choices.iter().find(|c| c.value == value) {
            return c.label.clone();
        }
        match self.display.as_str() {
            "ms" => format!("{value} ms"),
            "tenths_s" => format!("{:.1} s", value as f64 / 10.),
            "half_db" => format!("{:+.1} dB", value as f64 / 2.),
            "db" => format!("{value:+} dB"),
            "hz" => format!("{value} Hz"),
            "cm" => format!("{value} cm"),
            "percent" => format!("{value}%"),
            "double_percent" => format!("{}%", value * 2),
            "plus_one" => (value + 1).to_string(),
            "signed" => format!("{value:+}"),
            "off_negative" if value < 0 => "OFF".into(),
            _ => value.to_string(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Selector {
    pub pages: Vec<String>,
    pub order: Vec<usize>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ChainTable {
    pub pedal: i32,
    pub send_return: i32,
    pub flows: Vec<Vec<String>>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Assignment {
    pub knob_offset: usize,
    pub targets: Vec<Control>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Catalog {
    pub controls: BTreeMap<String, Control>,
    pub groups: BTreeMap<String, Vec<String>>,
    pub selectors: BTreeMap<String, Selector>,
    pub chains: Vec<ChainTable>,
    pub assignments: BTreeMap<String, Assignment>,
    pub sources: BTreeMap<String, String>,
}
impl Catalog {
    pub fn load() -> Self {
        serde_json::from_str(include_str!("../data/editor-ui.json")).expect("embedded UI bindings")
    }
    pub fn page(&self, selector: &str, value: i32) -> Option<&str> {
        let s = self.selectors.get(selector)?;
        let n = *s.order.get(usize::try_from(value).ok()?)?;
        s.pages.get(n).map(String::as_str)
    }
    pub fn group(&self, id: &str) -> Vec<Control> {
        self.groups
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.controls.get(id).cloned())
            .collect()
    }
    pub fn control(&self, id: &str) -> Option<Control> {
        self.controls.get(id).cloned()
    }
    pub fn assignment_key(&self, section: Section, s: &Schema, set: &ParamSet) -> Option<String> {
        match section {
            Section::Booster => Some("booster".into()),
            Section::Delay | Section::Delay2 => Some("delay".into()),
            Section::Reverb => Some("reverb".into()),
            Section::Mod | Section::Fx => {
                let color = *set.get("PATCH%COLOR")?.get(section.color_index()?)?;
                let bank = color as usize + if section == Section::Mod { 1 } else { 4 };
                let c = self
                    .control("modfx-mod-type-select-box")?
                    .remap("FX(1)", &format!("FX({bank})"));
                Some(
                    self.page("modfx-mod-type-select-box", c.value(s, set).ok()?)?
                        .strip_prefix("modfx-")?
                        .strip_suffix("-page")?
                        .into(),
                )
            }
            _ => None,
        }
    }
    pub fn knob_assignment(&self, key: &str, label: &str) -> Option<Control> {
        let assignment = self.assignments.get(key)?;
        let mut c = self.control(&format!("asgn-pedal-{key}-select-box"))?;
        c.block = "PATCH%ASSIGN_KNOBS".into();
        c.offset = assignment.knob_offset;
        c.id = format!("knob-{key}");
        c.label = label.into();
        Some(c)
    }
    pub fn pedal_assignment(
        &self,
        s: &Schema,
        set: &ParamSet,
        pedal: &str,
        expanded: bool,
    ) -> Result<Vec<Control>> {
        ensure!(
            ["pedal", "exp1", "exp2", "exp3"].contains(&pedal),
            "unknown expression input"
        );
        ensure!(
            !expanded || pedal != "pedal",
            "amp pedal cannot be expanded"
        );
        let remap = |c: Control| if expanded { c.remap("(1)", "(2)") } else { c };
        let mut function = remap(
            self.control(&format!("asgn-{pedal}-function-select-box"))
                .context("missing expression function")?,
        );
        function.label = "Function".into();
        let value = function.value(s, set)?;
        let section = match value {
            4 => Section::Booster,
            5 => Section::Mod,
            6 => Section::Fx,
            7 | 8 => Section::Delay,
            9 => Section::Reverb,
            _ => return Ok(vec![function]),
        };
        let key = self
            .assignment_key(section, s, set)
            .context("unknown assigned algorithm")?;
        let mut target = remap(
            self.control(&format!("asgn-{pedal}-{key}-select-box"))
                .context("missing target")?,
        );
        target.label = "Target".into();
        let choice = usize::try_from(target.value(s, set)?)?;
        let template = self
            .assignments
            .get(&key)
            .and_then(|a| a.targets.get(choice))
            .context("invalid expression target")?;
        let (min, max) = if template.block.is_empty() {
            (0, 100)
        } else {
            template.range(s)?
        };
        let mut result = vec![function, target];
        for (suffix, label) in [("min", "Heel"), ("max", "Toe")] {
            let mut c = remap(
                self.control(&format!("asgn-{pedal}-{key}-{suffix}-spinner"))
                    .context("missing expression range")?,
            );
            c.label = label.into();
            c.min = Some(min);
            c.max = Some(max);
            c.bias = template.bias;
            c.display = template.display.clone();
            c.choices = template.choices.clone();
            result.push(c);
        }
        Ok(result)
    }
    pub fn chain(
        &self,
        pattern: i32,
        pedal: i32,
        sr: i32,
        eq1: i32,
        eq2: i32,
    ) -> Result<Vec<String>> {
        ensure!(
            [0, 1].contains(&eq1) && [0, 1].contains(&eq2),
            "unsupported EQ position"
        );
        let t = self
            .chains
            .iter()
            .find(|c| c.pedal == pedal && c.send_return == sr)
            .context("unsupported routing values")?;
        let base = t
            .flows
            .get(usize::try_from(pattern)?)
            .context("unsupported chain pattern")?;
        let eq_flow = match (eq1, eq2) {
            (0, 0) => vec!["eq", "eq2", "amp"],
            (1, 1) => vec!["amp", "eq", "eq2"],
            (0, 1) => vec!["eq", "amp", "eq2"],
            _ => vec!["eq2", "amp", "eq"],
        };
        let mut chain = vec!["in".to_string()];
        for b in base {
            if b == "amp" {
                chain.extend(eq_flow.iter().map(|s| s.to_string()));
            } else {
                chain.push(b.clone());
            }
        }
        chain.push("speaker".into());
        Ok(chain)
    }
    /// Resolve a single-block move to firmware-supported routing parameters.
    pub fn move_in_chain(&self, current: &[String], source: &str, after: &str) -> Result<[i32; 5]> {
        ensure!(
            source != after && !["in", "speaker", "amp", "fv", "dly2", "rev"].contains(&source),
            "fixed block"
        );
        ensure!(after != "speaker", "cannot move after output");
        let mut moved = current.to_vec();
        let from = moved
            .iter()
            .position(|s| s == source)
            .context("unknown source")?;
        moved.remove(from);
        let to = moved
            .iter()
            .position(|s| s == after)
            .context("unknown destination")?;
        moved.insert(to + 1, source.into());
        ensure!(moved != current, "unchanged routing");
        for pattern in 0..9 {
            for pedal in 0..2 {
                for sr in 0..3 {
                    for eq1 in 0..2 {
                        for eq2 in 0..2 {
                            if self.chain(pattern, pedal, sr, eq1, eq2)? == moved {
                                return Ok([pattern, pedal, sr, eq1, eq2]);
                            }
                        }
                    }
                }
            }
        }
        anyhow::bail!("unsupported routing")
    }
    /// Last-read panel macro position, NOT an algorithm's effect level. The
    /// shipped watcher uses a +1 wire bias; zero on the wire means unavailable.
    /// TEMP macro writes are dedicated session commands, never raw schema edits.
    pub fn panel_position(&self, s: &Schema, set: &ParamSet, section: Section) -> Option<i32> {
        let offset = match section {
            Section::Booster => 0,
            Section::Mod => 1,
            Section::Fx => 2,
            Section::Delay => 3,
            Section::Reverb => 4,
            Section::Solo => 0,
            _ => return None,
        };
        let b = s
            .block(if section == Section::Solo {
                "PATCH%PATCH_KNOB_SOLO_DELAY_READONLY"
            } else {
                "PATCH%PATCH_KNOB_READONLY"
            })
            .ok()?;
        let p = b.parameters.iter().find(|p| p.offset == offset)?;
        let value = p.decode_number(set.get(&b.id)?).ok()?;
        (0..=100).contains(&value).then_some(value)
    }
    pub fn enabled(&self, id: &str, s: &Schema, set: &ParamSet, bank: usize) -> bool {
        let val = |key: &str| {
            self.control(key)
                .map(|c| c.remap("FX_DETAIL(1)", &format!("FX_DETAIL({bank})")))
                .and_then(|c| c.value(s, set).ok())
                .unwrap_or(0)
        };
        if id == "modfx-guitarsim-body-dial" {
            return val("modfx-guitarsim-type-select-box") >= 3;
        }
        if id.starts_with("modfx-pshift-")
            && ["mode2", "pitch2", "fine2", "pre-delay2", "level2"]
                .iter()
                .any(|p| id.contains(p))
        {
            return val("modfx-pshift-voice-select-box") != 0;
        }
        if id.starts_with("modfx-harmonist-") {
            if (id.contains("harmony2")
                || id.contains("level2")
                || id.contains("pre-delay2")
                || id.contains("-v2-"))
                && val("modfx-harmonist-voice-select-box") == 0
            {
                return false;
            }
            if id.contains("-v1-harm") {
                return val("modfx-harmonist-harmony1-dial") == 29;
            }
            if id.contains("-v2-harm") {
                return val("modfx-harmonist-harmony2-dial") == 29;
            }
        }
        if id == "modfx-humanizer-sens-dial" {
            return val("modfx-humanizer-mode-select-box") == 0;
        }
        if id == "modfx-humanizer-manual-dial" {
            return val("modfx-humanizer-mode-select-box") != 0;
        }
        if id == "modfx-dc30-cho-intensity-dial" {
            return val("modfx-dc30-type-select-box") == 0;
        }
        if id.starts_with("modfx-dc30-echo-") {
            return val("modfx-dc30-type-select-box") != 0;
        }
        true
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Amp,
    Chain,
    Pedal,
    Booster,
    Mod,
    Fx,
    Delay,
    Delay2,
    Reverb,
    Eq,
    Eq2,
    Noise,
    SendReturn,
    Solo,
    Contour,
    Assignments,
}
impl Section {
    pub const ALL: [Self; 16] = [
        Self::Amp,
        Self::Chain,
        Self::Pedal,
        Self::Booster,
        Self::Mod,
        Self::Fx,
        Self::Delay,
        Self::Delay2,
        Self::Reverb,
        Self::Eq,
        Self::Eq2,
        Self::Noise,
        Self::SendReturn,
        Self::Solo,
        Self::Contour,
        Self::Assignments,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Amp => "AMP",
            Self::Chain => "CHAIN",
            Self::Pedal => "PEDAL FX",
            Self::Booster => "BOOSTER",
            Self::Mod => "MOD",
            Self::Fx => "FX",
            Self::Delay => "DELAY",
            Self::Delay2 => "DELAY 2",
            Self::Reverb => "REVERB",
            Self::Eq => "EQ",
            Self::Eq2 => "EQ 2",
            Self::Noise => "NOISE SUPPRESSOR",
            Self::SendReturn => "SEND / RETURN",
            Self::Solo => "SOLO",
            Self::Contour => "CONTOUR",
            Self::Assignments => "ASSIGN",
        }
    }
    pub fn color_index(self) -> Option<usize> {
        match self {
            Self::Booster => Some(0),
            Self::Mod => Some(1),
            Self::Fx => Some(2),
            Self::Delay => Some(3),
            Self::Delay2 | Self::Reverb => Some(4),
            _ => None,
        }
    }
    pub fn switch(self) -> Option<&'static str> {
        match self {
            Self::Booster => Some("booster-sw-btn"),
            Self::Mod => Some("modfx-mod-sw-btn"),
            Self::Fx => Some("modfx-fx-sw-btn"),
            Self::Delay => Some("delay-delay1-sw-btn"),
            Self::Delay2 => Some("delay-delay2-sw-btn"),
            Self::Reverb => Some("reverb-sw-btn"),
            Self::Eq => Some("eq-sw-btn"),
            Self::Eq2 => Some("eq2-sw-btn"),
            Self::Pedal => Some("pedalfx-sw-btn"),
            Self::Noise => Some("ns-sw-btn"),
            Self::SendReturn => Some("sr-sw-btn"),
            Self::Solo => Some("solo-sw-btn"),
            _ => None,
        }
    }
}
pub fn model_name(code: u8) -> &'static str {
    match code {
        5 => "KATANA-50",
        6 => "KATANA-100",
        7 => "KATANA-100/212",
        8 => "KATANA-HEAD",
        9 => "KATANA Artist",
        10 => "KATANA-50 EX",
        11 => "KATANA Artist HEAD",
        _ => "Unknown",
    }
}
pub fn bloom(model: u8) -> bool {
    [8, 9, 11].contains(&model)
}
pub fn solo_delay(model: u8) -> bool {
    [9, 11].contains(&model)
}
pub fn gafc(model: u8) -> bool {
    model != 5
}
#[cfg(test)]
#[path = "editor/assignment_tests.rs"]
mod assignment_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Patch;
    #[test]
    fn all_metadata_bindings_resolve() {
        let s = Schema::load();
        let u = Catalog::load();
        assert_eq!(u.controls.len(), 1243);
        assert_eq!(u.assignments.len(), 34);
        for c in u.controls.values() {
            c.parameter(&s).unwrap();
        }
    }
    #[test]
    fn parameter_ui_roundtrip_does_not_touch_other_fields() {
        let s = Schema::load();
        let u = Catalog::load();
        let mut p = Patch::new(&s).unwrap();
        let before = p.parameters.clone();
        let c = u
            .control("booster-tone-dial")
            .unwrap()
            .remap("BOOSTER(1)", "BOOSTER(3)");
        let e = c.set(&s, &mut p.parameters, -12).unwrap();
        assert_eq!(e.block, "PATCH%BOOSTER(3)");
        assert_eq!(c.value(&s, &p.parameters).unwrap(), -12);
        assert_eq!(p.parameters["PATCH%BOOSTER(1)"], before["PATCH%BOOSTER(1)"]);
    }
    #[test]
    fn algorithm_pages_and_nonsequential_values() {
        let u = Catalog::load();
        assert_eq!(
            u.page("modfx-mod-type-select-box", 23),
            Some("modfx-chorus-page")
        );
        assert_eq!(
            u.page("delay-delay1-type-select-box", 7),
            Some("delay-sde3000-page")
        );
        assert_eq!(
            u.page("reverb-type-select-box", 3),
            Some("reverb-spring-page")
        );
        assert_eq!(u.controls["booster-type-select-box"].choices[0].value, 1);
        assert_eq!(u.controls["modfx-twah-mode-dial"].text(1), "BPF");
    }
    #[test]
    fn all_216_chain_combinations_have_same_unique_nodes() {
        let u = Catalog::load();
        for p in 0..9 {
            for pedal in 0..2 {
                for sr in 0..3 {
                    for eq1 in 0..2 {
                        for eq2 in 0..2 {
                            let c = u.chain(p, pedal, sr, eq1, eq2).unwrap();
                            let mut nodes = c.clone();
                            nodes.sort();
                            nodes.dedup();
                            assert_eq!(nodes.len(), 14);
                            assert_eq!(c.first().unwrap(), "in");
                            assert_eq!(c.last().unwrap(), "speaker");
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn chain_moves_only_resolve_exact_supported_routes() {
        let u = Catalog::load();
        let mut legal_moves = 0;
        for pattern in 0..9 {
            for pedal in 0..2 {
                for sr in 0..3 {
                    for eq1 in 0..2 {
                        for eq2 in 0..2 {
                            let chain = u.chain(pattern, pedal, sr, eq1, eq2).unwrap();
                            for source in &chain {
                                for after in &chain {
                                    if let Ok([p, pd, s, e1, e2]) =
                                        u.move_in_chain(&chain, source, after)
                                    {
                                        let mut expected = chain.clone();
                                        expected.remove(
                                            expected.iter().position(|id| id == source).unwrap(),
                                        );
                                        let to =
                                            expected.iter().position(|id| id == after).unwrap();
                                        expected.insert(to + 1, source.clone());
                                        assert_eq!(u.chain(p, pd, s, e1, e2).unwrap(), expected);
                                        legal_moves += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(legal_moves > 216);
        let chain = u.chain(0, 0, 0, 0, 0).unwrap();
        for source in ["in", "amp", "speaker", "fv", "rev", "dly2", "missing"] {
            assert!(u.move_in_chain(&chain, source, "bst").is_err());
        }
        assert!(u.move_in_chain(&chain, "bst", "speaker").is_err());
        assert!(u.move_in_chain(&chain, "bst", "bst").is_err());
    }
    #[test]
    fn viewing_algorithm_or_bank_does_not_reset_hidden_parameters() {
        let s = Schema::load();
        let u = Catalog::load();
        let p = Patch::new(&s).unwrap();
        let before = p.parameters.clone();
        for n in 1..=6 {
            for ty in 0..31 {
                let page = u.page("modfx-mod-type-select-box", ty).unwrap();
                for c in u.group(page) {
                    c.remap("FX_DETAIL(1)", &format!("FX_DETAIL({n})"))
                        .value(&s, &p.parameters)
                        .unwrap();
                }
            }
        }
        assert_eq!(p.parameters, before);
    }
}
