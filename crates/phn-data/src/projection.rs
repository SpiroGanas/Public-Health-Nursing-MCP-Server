//! NAD83 / Massachusetts Mainland (EPSG:26986) ↔ latitude and longitude.
//!
//! The bundled boundaries (see [`crate::boundaries`]) are stored in
//! EPSG:26986, a Lambert Conformal Conic projection with two standard
//! parallels on the GRS 1980 ellipsoid. Weather and air quality services work
//! in latitude and longitude, so the Phase 12 tools convert between the two.
//!
//! Formulas: J. P. Snyder, *Map Projections — A Working Manual*, USGS
//! Professional Paper 1395 (1987), equations 14-15, 15-9, 15-1 to 15-5 (forward)
//! and 14-4, 15-7, 15-8, 7-9 (inverse). Parameters: EPSG:26986 (standard
//! parallels 42°41′N and 41°43′N, origin 41°00′N 71°30′W, false easting
//! 200,000 m, false northing 750,000 m). NAD83 and WGS 84 differ by about a
//! metre, which does not matter at the scale these tools use.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

const A: f64 = 6_378_137.0;
const INV_F: f64 = 298.257_222_101;
const LAT1: f64 = 42.0 + 41.0 / 60.0;
const LAT2: f64 = 41.0 + 43.0 / 60.0;
const LAT0: f64 = 41.0;
const LON0: f64 = -71.5;
const FALSE_EASTING: f64 = 200_000.0;
const FALSE_NORTHING: f64 = 750_000.0;

struct Lcc {
    e: f64,
    n: f64,
    af: f64,
    rho0: f64,
}

fn eccentricity() -> f64 {
    let f = 1.0 / INV_F;
    (2.0 * f - f * f).sqrt()
}

fn m(phi: f64, e: f64) -> f64 {
    phi.cos() / (1.0 - (e * phi.sin()).powi(2)).sqrt()
}

fn t(phi: f64, e: f64) -> f64 {
    let es = e * phi.sin();
    (FRAC_PI_4 - phi / 2.0).tan() / ((1.0 - es) / (1.0 + es)).powf(e / 2.0)
}

fn params() -> Lcc {
    let e = eccentricity();
    let (p1, p2, p0) = (LAT1.to_radians(), LAT2.to_radians(), LAT0.to_radians());
    let (m1, m2) = (m(p1, e), m(p2, e));
    let (t1, t2, t0) = (t(p1, e), t(p2, e), t(p0, e));
    let n = (m1.ln() - m2.ln()) / (t1.ln() - t2.ln());
    let f = m1 / (n * t1.powf(n));
    let af = A * f;
    Lcc {
        e,
        n,
        af,
        rho0: af * t0.powf(n),
    }
}

/// Latitude and longitude (degrees) to EPSG:26986 metres (x east, y north).
pub fn to_mass_mainland(lat: f64, lon: f64) -> (f64, f64) {
    let p = params();
    let rho = p.af * t(lat.to_radians(), p.e).powf(p.n);
    let theta = p.n * (lon - LON0).to_radians();
    (
        FALSE_EASTING + rho * theta.sin(),
        FALSE_NORTHING + p.rho0 - rho * theta.cos(),
    )
}

/// EPSG:26986 metres to latitude and longitude (degrees).
pub fn from_mass_mainland(x: f64, y: f64) -> (f64, f64) {
    let p = params();
    let dx = x - FALSE_EASTING;
    let dy = p.rho0 - (y - FALSE_NORTHING);
    let rho = (dx * dx + dy * dy).sqrt() * p.n.signum();
    let tt = (rho / p.af).powf(1.0 / p.n);
    let theta = dx.atan2(dy);
    let lon = theta / p.n + LON0.to_radians();
    let mut phi = FRAC_PI_2 - 2.0 * tt.atan();
    for _ in 0..15 {
        let es = p.e * phi.sin();
        let next = FRAC_PI_2 - 2.0 * (tt * ((1.0 - es) / (1.0 + es)).powf(p.e / 2.0)).atan();
        if (next - phi).abs() < 1e-14 {
            phi = next;
            break;
        }
        phi = next;
    }
    (phi.to_degrees(), lon.to_degrees())
}

/// True when (x, y) is inside the polygon given as rings (exterior first,
/// holes after), using the even-odd rule.
pub fn point_in_rings(x: f64, y: f64, rings: &[Vec<(f64, f64)>]) -> bool {
    let mut inside = false;
    for ring in rings {
        let n = ring.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

/// Great-circle distance in kilometres (haversine, mean Earth radius).
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6_371.0088 * h.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_maps_to_false_easting_and_northing() {
        let (x, y) = to_mass_mainland(LAT0, LON0);
        assert!((x - FALSE_EASTING).abs() < 1e-6, "{x}");
        assert!((y - FALSE_NORTHING).abs() < 1e-6, "{y}");
    }

    #[test]
    fn round_trips_across_massachusetts() {
        for &(lat, lon) in &[
            (42.3601, -71.0589),
            (42.4501, -73.2454),
            (41.2835, -70.0995),
            (42.7262, -71.1909),
            (41.6362, -70.9342),
        ] {
            let (x, y) = to_mass_mainland(lat, lon);
            let (la, lo) = from_mass_mainland(x, y);
            assert!((la - lat).abs() < 1e-9 && (lo - lon).abs() < 1e-9, "{lat},{lon}");
        }
    }

    #[test]
    fn boston_lands_east_and_north_of_the_origin() {
        // Boston City Hall is about 36 km east of 71°30′W and about 150 km north
        // of 41°N, so its State Plane coordinates are near (236 km, 900 km).
        let (x, y) = to_mass_mainland(42.3601, -71.0589);
        assert!((230_000.0..242_000.0).contains(&x), "{x}");
        assert!((895_000.0..906_000.0).contains(&y), "{y}");
    }

    #[test]
    fn points_in_polygons_and_distances() {
        let square = vec![vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]];
        assert!(point_in_rings(5.0, 5.0, &square));
        assert!(!point_in_rings(15.0, 5.0, &square));
        let with_hole = vec![
            square[0].clone(),
            vec![(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)],
        ];
        assert!(!point_in_rings(5.0, 5.0, &with_hole));
        // Boston to Springfield is about 129 km.
        let d = distance_km((42.3601, -71.0589), (42.1015, -72.5898));
        assert!((d - 128.5).abs() < 6.0, "{d}");
    }
}
