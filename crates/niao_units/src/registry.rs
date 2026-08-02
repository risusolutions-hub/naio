use crate::dimension::Dimension;
use crate::error::{UnitError, UnitResult};
use crate::parse::parse_unit_expr;
use crate::unit::{Affine, Unit};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Registry {
    units: HashMap<String, Unit>,
}

impl Registry {
    pub fn new() -> Self {
        let mut reg = Self {
            units: HashMap::new(),
        };
        reg.load_defaults();
        reg
    }

    pub fn lookup(&self, name: &str) -> UnitResult<Unit> {
        let key = normalize_key(name);
        self.units
            .get(&key)
            .cloned()
            .ok_or_else(|| UnitError::UnknownUnit(name.to_string()))
    }

    pub fn define(&mut self, name: &str, unit: Unit) {
        self.units.insert(normalize_key(name), unit);
    }

    pub fn define_expr(&mut self, name: &str, expr: &str) -> UnitResult<()> {
        let unit = parse_unit_expr(expr, self)?;
        let mut u = unit;
        u.symbol = name.to_string();
        self.define(name, u);
        Ok(())
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.units.keys().cloned().collect();
        names.sort();
        names.dedup();
        names
    }

    pub fn prefixes(&self) -> Vec<&'static str> {
        vec![
            "yotta", "zetta", "exa", "peta", "tera", "giga", "mega", "kilo", "hecto", "deca",
            "deci", "centi", "milli", "micro", "nano", "pico", "femto", "atto", "zepto", "yocto",
            "Y", "Z", "E", "P", "T", "G", "M", "k", "h", "da", "d", "c", "m", "u", "n", "p", "f",
            "a", "z", "y",
        ]
    }

    fn load_defaults(&mut self) {
        let l = Dimension {
            l: 1,
            ..Default::default()
        };
        let m = Dimension {
            m: 1,
            ..Default::default()
        };
        let t = Dimension {
            t: 1,
            ..Default::default()
        };
        let i = Dimension {
            i: 1,
            ..Default::default()
        };
        let th = Dimension {
            th: 1,
            ..Default::default()
        };
        let n = Dimension {
            n: 1,
            ..Default::default()
        };
        let j = Dimension {
            j: 1,
            ..Default::default()
        };

        // Length
        self.add_unit("m", "meter", l, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("m", &["meter", "meters", "metre", "metres"]);
        self.add_scaled("km", "kilometer", l, 1000.0, &["kilometer", "kilometers"]);
        self.add_scaled("cm", "centimeter", l, 0.01, &["centimeter", "centimeters"]);
        self.add_scaled("mm", "millimeter", l, 0.001, &["millimeter", "millimeters"]);
        self.add_scaled(
            "um",
            "micrometer",
            l,
            1e-6,
            &["micrometer", "micrometers", "micron"],
        );
        self.add_scaled("nm", "nanometer", l, 1e-9, &["nanometer", "nanometers"]);
        self.add_scaled("inch", "inch", l, 0.0254, &["in", "inches"]);
        self.add_scaled("ft", "foot", l, 0.3048, &["foot", "feet"]);
        self.add_scaled("yard", "yard", l, 0.9144, &["yd", "yards"]);
        self.add_scaled("mile", "mile", l, 1609.344, &["mi", "miles"]);

        // Mass
        self.add_unit("kg", "kilogram", m, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("kg", &["kilogram", "kilograms"]);
        self.add_scaled("g", "gram", m, 0.001, &["gram", "grams"]);
        self.add_scaled("mg", "milligram", m, 1e-6, &["milligram", "milligrams"]);
        self.add_scaled("lb", "pound", m, 0.45359237, &["pound", "pounds", "lbs"]);
        self.add_scaled("oz", "ounce", m, 0.028349523125, &["ounce", "ounces"]);

        // Time
        self.add_unit("s", "second", t, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("s", &["sec", "second", "seconds"]);
        self.add_scaled(
            "ms",
            "millisecond",
            t,
            1e-3,
            &["millisecond", "milliseconds"],
        );
        self.add_scaled(
            "us",
            "microsecond",
            t,
            1e-6,
            &["microsecond", "microseconds"],
        );
        self.add_scaled("ns", "nanosecond", t, 1e-9, &["nanosecond", "nanoseconds"]);
        self.add_scaled("min", "minute", t, 60.0, &["minute", "minutes"]);
        self.add_scaled("h", "hour", t, 3600.0, &["hr", "hour", "hours"]);
        self.add_scaled("day", "day", t, 86400.0, &["days"]);

        // Current
        self.add_unit("A", "ampere", i, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("A", &["amp", "ampere", "amperes"]);
        self.add_scaled(
            "mA",
            "milliampere",
            i,
            1e-3,
            &["milliampere", "milliamperes"],
        );

        // Temperature (affine)
        self.add_unit("K", "kelvin", th, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("K", &["kelvin", "kelvins"]);
        self.add_affine(
            "degC",
            "celsius",
            th,
            1.0,
            273.15,
            &["celsius", "C", "celsius_degree"],
        );
        self.add_affine(
            "degF",
            "fahrenheit",
            th,
            5.0 / 9.0,
            459.67,
            &["fahrenheit", "F"],
        );

        // Amount
        self.add_unit("mol", "mole", n, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("mol", &["mole", "moles"]);

        // Luminous intensity
        self.add_unit("cd", "candela", j, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("cd", &["candela", "candelas"]);

        // Derived
        let n_dim = l.mul(m).div(t).div(t); // N = kg*m/s^2
        self.add_unit("N", "newton", n_dim, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("N", &["newton", "newtons"]);
        self.add_scaled(
            "kN",
            "kilonewton",
            n_dim,
            1000.0,
            &["kilonewton", "kilonewtons"],
        );

        let pa = n_dim.div(l); // Pa = N/m^2
        self.add_unit("Pa", "pascal", pa, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("Pa", &["pascal", "pascals"]);
        self.add_scaled(
            "kPa",
            "kilopascal",
            pa,
            1000.0,
            &["kilopascal", "kilopascals"],
        );
        self.add_scaled("MPa", "megapascal", pa, 1e6, &["megapascal", "megapascals"]);
        self.add_scaled("bar", "bar", pa, 1e5, &["bars"]);
        self.add_scaled(
            "atm",
            "atmosphere",
            pa,
            101325.0,
            &["atmosphere", "atmospheres"],
        );
        self.add_scaled("psi", "psi", pa, 6894.757293168, &[]);

        let j_dim = n_dim.mul(l); // J = N*m
        self.add_unit("J", "joule", j_dim, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("J", &["joule", "joules"]);
        self.add_scaled(
            "kJ",
            "kilojoule",
            j_dim,
            1000.0,
            &["kilojoule", "kilojoules"],
        );
        self.add_scaled("cal", "calorie", j_dim, 4.184, &["calorie", "calories"]);
        self.add_scaled(
            "kcal",
            "kilocalorie",
            j_dim,
            4184.0,
            &["kilocalorie", "kilocalories"],
        );
        self.add_scaled(
            "Wh",
            "watt_hour",
            j_dim,
            3600.0,
            &["watt_hour", "watt_hours"],
        );
        self.add_scaled(
            "kWh",
            "kilowatt_hour",
            j_dim,
            3.6e6,
            &["kilowatt_hour", "kilowatt_hours"],
        );

        let w_dim = j_dim.div(t);
        self.add_unit("W", "watt", w_dim, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("W", &["watt", "watts"]);
        self.add_scaled("kW", "kilowatt", w_dim, 1000.0, &["kilowatt", "kilowatts"]);
        self.add_scaled("hp", "horsepower", w_dim, 745.69987158227, &["horsepower"]);

        let hz = Dimension {
            t: -1,
            ..Default::default()
        };
        self.add_unit("Hz", "hertz", hz, 1.0, Affine::MULTIPLICATIVE);
        self.add_aliases("Hz", &["hertz"]);
        self.add_scaled("kHz", "kilohertz", hz, 1000.0, &["kilohertz"]);
        self.add_scaled("MHz", "megahertz", hz, 1e6, &["megahertz"]);

        let liter_dim = l.pow(3).unwrap();
        self.add_scaled(
            "L",
            "liter",
            liter_dim,
            0.001,
            &["l", "liter", "liters", "litre", "litres"],
        );
        self.add_scaled(
            "mL",
            "milliliter",
            liter_dim,
            1e-6,
            &["ml", "milliliter", "milliliters"],
        );

        // Velocity aliases via define_expr
        let _ = self.define_expr("mph", "mile/hour");
        let _ = self.define_expr("kmph", "km/h");
        let _ = self.define_expr("kph", "km/h");
    }

    fn add_unit(
        &mut self,
        symbol: &str,
        _canonical: &str,
        dimension: Dimension,
        scale: f64,
        affine: Affine,
    ) {
        let unit = Unit {
            dimension,
            scale,
            affine,
            symbol: symbol.to_string(),
        };
        self.units.insert(normalize_key(symbol), unit);
    }

    fn add_aliases(&mut self, symbol: &str, aliases: &[&str]) {
        let base = self.lookup(symbol).expect("base unit must exist");
        for a in aliases {
            self.units.insert(normalize_key(a), base.clone());
        }
    }

    fn add_scaled(
        &mut self,
        symbol: &str,
        canonical: &str,
        dimension: Dimension,
        scale: f64,
        aliases: &[&str],
    ) {
        self.add_unit(symbol, canonical, dimension, scale, Affine::MULTIPLICATIVE);
        self.add_aliases(symbol, aliases);
    }

    fn add_affine(
        &mut self,
        symbol: &str,
        canonical: &str,
        dimension: Dimension,
        scale: f64,
        offset: f64,
        aliases: &[&str],
    ) {
        self.add_unit(symbol, canonical, dimension, 1.0, Affine { scale, offset });
        self.add_aliases(symbol, aliases);
    }
}

fn normalize_key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_quantity;

    #[test]
    fn default_registry_has_meter() {
        let reg = Registry::default();
        assert!(reg.lookup("meter").is_ok());
    }

    #[test]
    fn mph_conversion() {
        let reg = Registry::default();
        let (n, u) = parse_quantity("60 mph", &reg).unwrap();
        assert!((n - 60.0).abs() < 1e-9);
        assert!(u.dimension.compatible(&Dimension {
            l: 1,
            t: -1,
            ..Default::default()
        }));
    }
}
