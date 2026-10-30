//! `cargo bench -p threebcad-core`: full rebuild, the cached rebuild after editing the last
//! feature, and tessellation, on the reference parts.

#[path = "../tests/parts/corpus.rs"]
mod corpus;

use criterion::{criterion_group, criterion_main, Criterion};
use threebcad_core::{End, FeatureKind};
use threebcad_kernel::mesher::{print_mesh, render_mesh};

fn benches(c: &mut Criterion) {
    let mut g = c.benchmark_group("rebuild");
    g.sample_size(10);
    for (name, build) in corpus::all() {
        g.bench_function(format!("full/{name}"), |b| {
            let mut doc = build();
            b.iter(|| doc.force_rebuild());
        });
    }
    // Plate: change the counterbore's end condition (the last feature) back and forth.
    g.bench_function("edit_last/plate_holes", |b| {
        let mut doc = corpus::plate_holes();
        let last = doc.history().iter().last().unwrap().0;
        let mut flip = false;
        b.iter(|| {
            flip = !flip;
            doc.edit(|h| {
                if let Some(FeatureKind::Hole(hole)) = h.get_mut(last).map(|f| &mut f.kind) {
                    hole.end = if flip { End::Blind(4.0) } else { End::ThroughAll };
                }
            });
        });
    });
    g.finish();

    let mut g = c.benchmark_group("tessellate");
    g.sample_size(10);
    let doc = corpus::flange();
    let solid = doc.bodies()[0].solid.clone();
    g.bench_function("render/flange", |b| b.iter(|| render_mesh(&solid, 0.05)));
    g.bench_function("print/flange", |b| b.iter(|| print_mesh(&solid, 0.01)));
    g.finish();
}

criterion_group!(rebuild, benches);
criterion_main!(rebuild);
