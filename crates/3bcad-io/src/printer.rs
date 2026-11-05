//! Printer build volumes, for laying a part out on the bed and warning when it does not fit.

/// A printer and its build volume (mm): width (X), depth (Y) and height (Z).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Printer {
    pub name: &'static str,
    pub bed: [f64; 3],
}

pub const PRINTERS: [Printer; 8] = [
    Printer {
        name: "Bambu Lab X1 / P1",
        bed: [256.0, 256.0, 256.0],
    },
    Printer {
        name: "Bambu Lab A1",
        bed: [256.0, 256.0, 256.0],
    },
    Printer {
        name: "Bambu Lab A1 mini",
        bed: [180.0, 180.0, 180.0],
    },
    Printer {
        name: "Prusa MK4",
        bed: [250.0, 210.0, 220.0],
    },
    Printer {
        name: "Prusa MINI",
        bed: [180.0, 180.0, 180.0],
    },
    Printer {
        name: "Prusa XL",
        bed: [360.0, 360.0, 360.0],
    },
    Printer {
        name: "Creality Ender 3",
        bed: [220.0, 220.0, 250.0],
    },
    Printer {
        name: "Voron 2.4 (350)",
        bed: [350.0, 350.0, 340.0],
    },
];

/// How far a part of `size` sticks out of the build volume `bed` on each axis (0 where it fits).
pub fn overflow(size: [f64; 3], bed: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| (size[i] - bed[i]).max(0.0))
}
