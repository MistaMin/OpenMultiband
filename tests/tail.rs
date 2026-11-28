//! ST-7: the reported tail covers the slowest ring-down (every crossover at 20 Hz, 48 dB/oct, 6 bands).

use openmultiband::OpenMultiband;
use openplugin::core::{Plugin, Tail};

#[test]
fn reported_tail_covers_the_worst_crossover_ring() {
    for sr in [44_100.0, 96_000.0] {
        let mut p = OpenMultiband::default();
        p.prepare(sr, 512, 2);
        p.set_param("bandCount", 5.0);
        p.set_param("xoverSlope", 2.0);
        for k in 1..=5 {
            p.set_param(&format!("xover{k}"), 20.0);
        }
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / 8_388_608.0 - 1.0
        };
        let (mut l, mut r) = (vec![0f32; 512], vec![0f32; 512]);
        for _ in 0..sr as usize / 512 {
            l.iter_mut().chain(r.iter_mut()).for_each(|v| *v = noise());
            p.process(&mut [&mut l[..], &mut r[..]], 512);
        }
        let Tail::Finite(tail) = p.tail() else { panic!("tail is not finite") };
        let quiet_after = p.latency() + tail as usize;
        let mut t = 0;
        while t < quiet_after + sr as usize {
            l.fill(0.0);
            r.fill(0.0);
            p.process(&mut [&mut l[..], &mut r[..]], 512);
            for (i, v) in l.iter().chain(&r).enumerate() {
                let at = t + i % 512;
                assert!(at < quiet_after || v.abs() <= 3.2e-5, "{sr} Hz: {v} at {at}, tail ends at {quiet_after}");
                assert!(*v == 0.0 || v.is_normal(), "{sr} Hz: denormal {v} at {at}");
            }
            t += 512;
        }
    }
}
