use bitvec::prelude::*;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use fcars::FormalContext;

fn patterned_context(objects_len: usize, attributes_len: usize) -> FormalContext<usize, usize> {
    let objects = (0..objects_len).collect::<Vec<_>>();
    let attributes = (0..attributes_len).collect::<Vec<_>>();
    let relation = (0..objects_len)
        .map(|object| {
            (0..attributes_len)
                .map(|attribute| {
                    let x = object.wrapping_mul(31) ^ attribute.wrapping_mul(17);
                    x % 11 < 7 || (object + attribute) % 13 == 0
                })
                .collect::<BitVec>()
        })
        .collect::<Vec<_>>();

    FormalContext::new(objects, attributes, relation)
}

fn bench_pcbo_count(c: &mut Criterion) {
    let mask_context = patterned_context(24, 24);
    let dense_context = patterned_context(129, 24);

    c.bench_function("pcbo_count_mask_24x24", |b| {
        b.iter(|| black_box(&mask_context).num_concepts())
    });

    c.bench_function("pcbo_count_dense_129x24", |b| {
        b.iter(|| black_box(&dense_context).num_concepts())
    });
}

fn bench_pcbo_materialize(c: &mut Criterion) {
    let mask_context = patterned_context(18, 18);
    let dense_context = patterned_context(129, 18);

    c.bench_function("pcbo_raw_concepts_mask_18x18", |b| {
        b.iter(|| black_box(&mask_context).all_concepts_raw())
    });

    c.bench_function("pcbo_concepts_mask_18x18", |b| {
        b.iter(|| black_box(&mask_context).all_concepts())
    });

    c.bench_function("pcbo_raw_concepts_dense_129x18", |b| {
        b.iter(|| black_box(&dense_context).all_concepts_raw())
    });
}

criterion_group!(benches, bench_pcbo_count, bench_pcbo_materialize);
criterion_main!(benches);
