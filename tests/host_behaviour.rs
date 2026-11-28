//! ST-9: host behaviour the conformance kit does not drive, plus NaN recovery and large-block dips.

use openmultiband::OpenMultiband;
use openplugin::prelude::*;

fn stereo(frames: usize, value: f32) -> Vec<Vec<f32>> {
    vec![vec![value; frames]; 2]
}

fn run(plugin: &mut OpenMultiband, buffers: &mut [Vec<f32>], frames: usize) {
    let mut io: Vec<&mut [f32]> = buffers.iter_mut().map(|c| c.as_mut_slice()).collect();
    plugin.process(&mut io, frames);
}

fn finite(buffers: &[Vec<f32>]) -> bool {
    buffers.iter().flatten().all(|s| s.is_finite())
}

#[test]
fn process_before_prepare_is_defined() {
    let mut plugin = OpenMultiband::default();
    let mut buffers = stereo(64, 0.5);
    run(&mut plugin, &mut buffers, 64);
    assert!(finite(&buffers));
}

#[test]
fn a_rate_change_without_a_reset_keeps_working() {
    let mut plugin = OpenMultiband::default();
    plugin.prepare(44100.0, 512, 2);
    run(&mut plugin, &mut stereo(512, 0.25), 512);
    plugin.prepare(96000.0, 512, 2); // a device switch: no reset in between
    for _ in 0..200 {
        let mut buffers = stereo(512, 0.25);
        run(&mut plugin, &mut buffers, 512);
        assert!(finite(&buffers));
    }
}

#[test]
fn every_frame_count_up_to_the_buffer_length_works() {
    let mut plugin = OpenMultiband::default();
    plugin.prepare(48000.0, 512, 2);
    for frames in [1usize, 2, 3, 7, 64, 333, 511, 512] {
        let mut buffers = stereo(512, 0.1);
        run(&mut plugin, &mut buffers, frames);
        assert!(finite(&buffers), "frames = {frames}");
    }
}

#[test]
fn nan_input_does_not_poison_crossover_or_autogain() {
    let mut plugin = OpenMultiband::default();
    plugin.set_param("autoGain", 1.0);
    plugin.prepare(48000.0, 256, 2);
    let mut bad = stereo(256, 0.2);
    bad[0][10] = f32::NAN;
    bad[1][20] = f32::INFINITY;
    run(&mut plugin, &mut bad, 256);
    assert!(finite(&bad));
    let mut good = vec![
        (0..512).map(|i| 0.25 * (i as f32 * 0.05).sin()).collect::<Vec<_>>(),
        (0..512).map(|i| 0.25 * (i as f32 * 0.05).sin()).collect::<Vec<_>>(),
    ];
    for _ in 0..8 {
        let mut blk = good.clone();
        run(&mut plugin, &mut blk, 512);
        assert!(finite(&blk));
        good = blk;
    }
    assert!(good[0].iter().any(|v| v.abs() > 0.01), "output silenced after NaN");
}

#[test]
fn zero_latency_switch_completes_inside_a_large_block() {
    let mut p = OpenMultiband::default();
    p.prepare(48_000.0, 4096, 2);
    let tone = |i: usize| 0.25 * (i as f32 * 0.05).sin();
    let mut bufs = vec![(0..4096).map(tone).collect::<Vec<_>>(), (0..4096).map(tone).collect::<Vec<_>>()];
    run(&mut p, &mut bufs, 4096);
    assert!(p.latency() > 0);
    p.set_param("zeroLatency", 1.0);
    let mut bufs = vec![(0..4096).map(tone).collect::<Vec<_>>(), (0..4096).map(tone).collect::<Vec<_>>()];
    run(&mut p, &mut bufs, 4096);
    assert_eq!(p.latency(), 0);
    let silent = bufs[0].iter().filter(|v| **v == 0.0).count();
    assert!(silent < 600 && bufs[0][3000..].iter().any(|v| v.abs() > 0.01), "{silent} silent samples");
}
