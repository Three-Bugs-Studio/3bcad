//! Part materials: density for mass properties and a colour for the graphics area and exports.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Material {
    pub name: String,
    /// g/cm³.
    pub density: f64,
    /// sRGB.
    pub color: [u8; 3],
}

impl Material {
    fn new(name: &str, density: f64, color: [u8; 3]) -> Self {
        Self {
            name: name.into(),
            density,
            color,
        }
    }

    /// Mass in grams of a volume in mm³.
    pub fn mass(&self, volume_mm3: f64) -> f64 {
        volume_mm3 * self.density / 1000.0
    }
}

impl Default for Material {
    fn default() -> Self {
        Self::new("PLA", 1.24, [179, 189, 204])
    }
}

/// The built-in library. Custom materials are stored in the part that uses them.
pub fn library() -> Vec<Material> {
    vec![
        Material::default(),
        Material::new("PETG", 1.27, [96, 160, 200]),
        Material::new("ABS", 1.04, [230, 230, 220]),
        Material::new("ASA", 1.07, [200, 200, 205]),
        Material::new("TPU", 1.21, [60, 60, 66]),
        Material::new("Nylon (PA12)", 1.01, [235, 232, 220]),
        Material::new("PC", 1.20, [210, 220, 230]),
        Material::new("Resin (standard)", 1.12, [150, 150, 158]),
        Material::new("Aluminium 6061", 2.70, [190, 195, 200]),
        Material::new("Steel (mild)", 7.85, [150, 155, 160]),
        Material::new("Stainless steel 304", 8.00, [175, 178, 182]),
        Material::new("Brass", 8.50, [205, 170, 80]),
        Material::new("Copper", 8.96, [200, 120, 80]),
        Material::new("Titanium Ti-6Al-4V", 4.43, [160, 160, 170]),
        Material::new("Oak", 0.75, [170, 125, 75]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_densities() {
        let lib = library();
        let steel = lib.iter().find(|m| m.name.starts_with("Steel")).unwrap();
        // A 10 mm steel cube weighs 7.85 g.
        assert!((steel.mass(1000.0) - 7.85).abs() < 1e-12);
        assert!(lib.iter().all(|m| m.density > 0.0));
        let mut names: Vec<_> = lib.iter().map(|m| &m.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lib.len());
    }
}
