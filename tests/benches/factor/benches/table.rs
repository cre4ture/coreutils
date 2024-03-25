// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
use array_init::array_init;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use num_prime::ExactRoots;
use number_theory::NumberTheory;
use std::{collections::BTreeMap, mem};
use num_bigint::BigUint;
use num_traits::FromPrimitive;

use uu_factor::{factor, Factors};

fn table(c: &mut Criterion) {
    #[cfg(target_os = "linux")]
    check_personality();

    const INPUT_SIZE: usize = 128;

    let inputs = {
        // Deterministic RNG; use an explicitly-named RNG to guarantee stability
        use rand::{RngCore, SeedableRng};
        use rand_chacha::ChaCha8Rng;
        const SEED: u64 = 0xdead_bebe_ea75_cafe; // spell-checker:disable-line
        let mut rng = ChaCha8Rng::seed_from_u64(SEED);

        std::iter::repeat_with(move || array_init::<_, _, INPUT_SIZE>(|_| rng.next_u64()))
    };

    let mut group = c.benchmark_group("table");
    group.throughput(Throughput::Elements(INPUT_SIZE as _));
    for (i, a) in inputs.take(10).enumerate() {
        let a_str = format!("#{:?}", i);
        group.bench_with_input(BenchmarkId::new("factor-uutils", &a_str), &a, |b, &a| {
            b.iter(|| {
                for n in a {
                    let _r = factor(n);
                }
            });
        });
        //group.bench_with_input(BenchmarkId::new("factor-number-theory", &a_str), &a, |b, &a| {
        //    b.iter(|| {
        //        for n in a {
        //            let _r = number_theory::Mpz::factor(&number_theory::Mpz::from_u64(n));
        //        }
        //    });
        //});
        //group.bench_with_input(BenchmarkId::new("factor-primal", &a_str), &a, |b, &a| {
        //    b.iter(|| {
        //        for n in a {
        //            let _r = primal::Sieve::new((n as f64).sqrt() as usize).factor(n as usize);
        //        }
        //    });
        //});
        group.bench_with_input(BenchmarkId::new("factor-num_prime", &a_str), &a, |b, &a| {
            b.iter(|| {
                for n in a {
                    let _r = num_prime::nt_funcs::factors(BigUint::from_u64(n).unwrap(), None);
                }
            });
        });
        // group.bench_with_input(BenchmarkId::new("factor-slow_primes", &a_str), &a, |b, &a| {
        //     b.iter(|| {
        //         for n in a {
        //             let _r = slow_primes::Primes::sieve((n as f64).sqrt() as usize).factor(n as usize);
        //         }
        //     });
        // });
        // group.bench_with_input(BenchmarkId::new("factor-factor-rs", &a_str), &a, |b, &a| {
        //     b.iter(|| {
        //         for n in a {
        //             let _r: Vec<_> = factor_rs::Fraction::whole(n).factorize().collect();
        //         }
        //     });
        // });
        // group.bench_with_input(BenchmarkId::new("factor-primeshor", &a_str), &a, |b, &a| {
        //     b.iter(|| {
        //         for n in a {
        //             let _r = primeshor::factorize(BigUint::from_u64(n).unwrap()).unwrap();
        //         }
        //     });
        // });
        // group.bench_with_input(BenchmarkId::new("factor-prime-factor", &a_str), &a, |b, &a| {
        //     b.iter(|| {
        //         for n in a {
        //             let _r = primefactor::PrimeFactors::from(n as u128).to_factor_vec();
        //         }
        //     });
        // });
        // group.bench_with_input(BenchmarkId::new("factor-prime_factorization", &a_str), &a, |b, &a| {
        //     b.iter(|| {
        //         for n in a {
        //             let _r = prime_factorization::Factorization::run(n);
        //         }
        //     });
        // });
    }
    group.finish();
}

#[cfg(target_os = "linux")]
fn check_personality() {
    use std::fs;
    const ADDR_NO_RANDOMIZE: u64 = 0x0040000;
    const PERSONALITY_PATH: &str = "/proc/self/personality";

    let p_string = fs::read_to_string(PERSONALITY_PATH)
        .unwrap_or_else(|_| panic!("Couldn't read '{}'", PERSONALITY_PATH))
        .strip_suffix('\n')
        .unwrap()
        .to_owned();

    let personality = u64::from_str_radix(&p_string, 16)
        .unwrap_or_else(|_| panic!("Expected a hex value for personality, got '{:?}'", p_string));
    if personality & ADDR_NO_RANDOMIZE == 0 {
        eprintln!(
            "WARNING: Benchmarking with ASLR enabled (personality is {:x}), results might not be reproducible.",
            personality
        );
    }
}

criterion_group!(benches, table);
criterion_main!(benches);
