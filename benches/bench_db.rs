// my_dex/benches/bench_db.rs

use criterion::{criterion_group, criterion_main, Criterion};
use my_dex::storage::dex_db::DexDB; // passe den Pfad an, falls n�tig

fn bench_db_put(c: &mut Criterion) {
    let db = DexDB::open_with_retries("tmp_db", 3, 1).expect("DB sollte ge�ffnet werden");
    c.bench_function("db put", |b| {
        b.iter(|| {
            db.put("benchmark_key", b"benchmark_value").expect("Put sollte funktionieren");
        })
    });
}

criterion_group!(benches, bench_db_put);
criterion_main!(benches);
