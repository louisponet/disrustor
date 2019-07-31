use disrustor::prelude::*;
use disrustor::BlockingWaitStrategy;
use disrustor::{BatchEventProcessor, RingBuffer, SingleProducerSequencer, SpinLoopWaitStrategy};
use log::*;
use std::sync::Arc;

const MAX: i64 = 200i64;

fn follow_sequence<W: WaitStrategy + 'static>() {
    let data: Arc<RingBuffer<u32>> = Arc::new(RingBuffer::new(128));
    let mut sequencer = SingleProducerSequencer::new(data.buffer_size(), W::new());

    let barrier1 = sequencer.create_barrier(vec![sequencer.get_cursor()]);
    let processor1 = BatchEventProcessor::create_mut(|data, sequence, _| {
        let val = *data;
        if val as i64 != sequence {
            panic!(
                "concurrency problem detected (p1), expected {}, but got {}",
                sequence, val
            );
        }
        debug!("updating sequence {} from {} to {}", sequence, val, val * 2);
        *data = val * 2;
    });

    let barrier2 = sequencer.create_barrier(vec![processor1.get_cursor()]);
    let processor2 = BatchEventProcessor::create(|data, sequence, _| {
        let val = *data;
        if val as i64 != sequence * 2 {
            panic!(
                "concurrency problem detected (p2), expected {}, but got {}",
                sequence * 2,
                val
            );
        }
    });

    sequencer.add_gating_sequence(processor1.get_cursor());
    sequencer.add_gating_sequence(processor2.get_cursor());

    let dp1 = data.clone();
    let t1 = std::thread::spawn(move || {
        processor1.run(barrier1, dp1.as_ref());
    });

    let dp2 = data.clone();
    let t2 = std::thread::spawn(move || {
        processor2.run(barrier2, dp2.as_ref());
    });

    for i in 1..=MAX / 20 {
        let range = ((i - 1) * 20)..=((i - 1) * 20 + 19);
        let items: Vec<_> = range.collect();
        sequencer.write(data.as_ref(), items, |d, _, v| {
            *d = *v as u32;
        });
    }

    sequencer.drain();
    t1.join().unwrap();
    t2.join().unwrap();
}

fn main() {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{:?}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                std::thread::current().id(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .chain(fern::log_file("output.log").unwrap())
        .apply()
        .unwrap();

    info!("running blocking wait strategy");
    follow_sequence::<BlockingWaitStrategy>();

    info!("running spinning wait strategy");
    follow_sequence::<SpinLoopWaitStrategy>();
}
