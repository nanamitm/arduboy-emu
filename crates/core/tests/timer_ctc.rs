use arduboy_core::Arduboy;

fn timer(top: u16) -> Arduboy {
    let mut a = Arduboy::new();
    a.write_data(0x99, (top >> 8) as u8);
    a.write_data(0x98, top as u8);
    a.write_data(0x91, 9); // CTC, no prescaler
    a
}

#[test]
fn ctc_update_batch_size_does_not_change_counter_or_pending_match() {
    for top in [0, 1, 9, 999, 65535] {
        let mut fine = timer(top);
        for tick in 1..=131073 {
            fine.timer3.update(tick, &mut fine.mem.data);
            if [1, 9, 10, 100, 1000, 65536, 131073].contains(&tick) {
                let mut batch = timer(top);
                batch.timer3.update(tick, &mut batch.mem.data);
                let f = fine.timer3.save_state();
                let b = batch.timer3.save_state();
                assert_eq!(f.tcnt, b.tcnt, "top={top}, tick={tick}");
                assert_eq!(f.ocf_a, b.ocf_a, "top={top}, tick={tick}");
                assert_eq!(f.tov, b.tov, "top={top}, tick={tick}");
            }
        }
    }
}

#[test]
fn ctc_holds_top_until_the_following_clock() {
    let mut a = timer(9);
    a.timer3.update(9, &mut a.mem.data);
    assert_eq!(a.timer3.save_state().tcnt, 9);
    assert_eq!(a.timer3.save_state().ocf_a, 1);
    a.write_data(0x38, 2); // clear OCF3A
    a.timer3.update(10, &mut a.mem.data);
    assert_eq!(a.timer3.save_state().tcnt, 0);
    assert_eq!(a.timer3.save_state().ocf_a, 0);
}

#[test]
fn lowering_top_below_counter_wraps_before_matching() {
    let mut a = timer(999);
    a.timer3.update(100, &mut a.mem.data);
    a.write_data(0x99, 0);
    a.write_data(0x98, 9);
    a.timer3.update(65545, &mut a.mem.data);
    let s = a.timer3.save_state();
    assert_eq!(s.tcnt, 9);
    assert_eq!(s.ocf_a, 1);
    assert_eq!(s.tov, 1);
}
