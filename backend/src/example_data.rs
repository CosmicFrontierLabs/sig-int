use shared::temporal::TimeRange;
use shared::types::{SignalKind, SignalState, Transition};

use crate::signal_source::{GroupSource, InlineSource};

/// Build a demo scene: three spaced I2C transactions at the system level.
pub fn build_i2c_example() -> Box<GroupSource> {
    let txn1 = build_i2c_transaction(
        "i2c_txn_1",
        0,
        "Write 0x48: set exposure",
        "#58a6ff", // blue
    );

    let txn2 = build_i2c_transaction(
        "i2c_txn_2",
        500_000, // 500μs gap
        "Write 0x48: set gain",
        "#3fb950", // green
    );

    let txn3 = build_i2c_transaction(
        "i2c_txn_3",
        1_200_000, // 1.2ms
        "Read 0x52: status",
        "#d2a8ff", // purple
    );

    Box::new(GroupSource {
        name: "I2C Bus".to_string(),
        path: "i2c".to_string(),
        time_span: TimeRange::new(0, 2_000_000), // 2ms total
        block_threshold: 50_000,                 // collapse when > 50μs/px (very zoomed out)
        block_color: "#f0883e".to_string(),
        block_label: "I2C Bus Activity".to_string(),
        children: vec![txn1, txn2, txn3],
    })
}

/// Build a single I2C transaction group with SCL, SDA, and decoded signals.
fn build_i2c_transaction(name: &str, start_ns: u64, label: &str, color: &str) -> Box<GroupSource> {
    let clock_period_ns = 10_000; // 100kHz I2C = 10μs period
    let half_period = clock_period_ns / 2;
    let num_bits: u64 = 27; // START + 7addr + RW + ACK + 8data + ACK + 8data + ACK + STOP ≈ 27 clocks
    let duration = clock_period_ns * num_bits;

    // SCL: clock signal
    let mut scl_transitions = Vec::new();
    scl_transitions.push(Transition {
        time_ns: start_ns,
        state: SignalState::High,
    });
    // START condition: SCL stays high, SDA drops
    let data_start = start_ns + half_period;
    for i in 0..num_bits {
        let t = data_start + i * clock_period_ns;
        scl_transitions.push(Transition {
            time_ns: t,
            state: SignalState::Low,
        });
        scl_transitions.push(Transition {
            time_ns: t + half_period,
            state: SignalState::High,
        });
    }

    let scl = Box::new(InlineSource {
        name: "SCL".to_string(),
        path: format!("{}.scl", name),
        kind: SignalKind::Clock,
        transitions: scl_transitions,
    });

    // SDA: data signal — simplified pattern
    let sda_transitions = build_sda_pattern(start_ns, clock_period_ns, num_bits);
    let sda = Box::new(InlineSource {
        name: "SDA".to_string(),
        path: format!("{}.sda", name),
        kind: SignalKind::Wire,
        transitions: sda_transitions,
    });

    // Decoded: show data bytes as labeled blocks
    let decoded = Box::new(InlineSource {
        name: "decoded".to_string(),
        path: format!("{}.decoded", name),
        kind: SignalKind::Bus { width: 8 },
        transitions: build_decoded_transitions(start_ns, clock_period_ns),
    });

    Box::new(GroupSource {
        name: name.to_string(),
        path: name.to_string(),
        time_span: TimeRange::new(start_ns, start_ns + duration),
        // Collapse to colored block when resolution > 1μs/px
        block_threshold: 1_000,
        block_color: color.to_string(),
        block_label: label.to_string(),
        children: vec![scl, sda, decoded],
    })
}

/// Generate a representative SDA pattern for an I2C transaction.
fn build_sda_pattern(start_ns: u64, clock_period_ns: u64, _num_bits: u64) -> Vec<Transition> {
    let half = clock_period_ns / 2;
    let mut t = Vec::new();

    // Idle high
    t.push(Transition {
        time_ns: start_ns,
        state: SignalState::High,
    });

    // START: SDA falls while SCL is high
    t.push(Transition {
        time_ns: start_ns + half / 2,
        state: SignalState::Low,
    });

    // Bits: alternate somewhat randomly to look realistic
    // Address: 0x48 = 0b1001000 + Write(0) = 10010000
    let address_bits = [true, false, false, true, false, false, false, false];
    let data1_bits = [false, false, false, false, true, true, true, true]; // 0x0F
    let data2_bits = [false, false, true, true, true, true, false, false]; // 0x3C

    let mut bit_idx = 0u64;
    let bit_start = start_ns + half; // after START

    // Address byte
    for &bit in &address_bits {
        let bt = bit_start + bit_idx * clock_period_ns;
        t.push(Transition {
            time_ns: bt,
            state: if bit {
                SignalState::High
            } else {
                SignalState::Low
            },
        });
        bit_idx += 1;
    }

    // ACK (low)
    t.push(Transition {
        time_ns: bit_start + bit_idx * clock_period_ns,
        state: SignalState::Low,
    });
    bit_idx += 1;

    // Data byte 1
    for &bit in &data1_bits {
        let bt = bit_start + bit_idx * clock_period_ns;
        t.push(Transition {
            time_ns: bt,
            state: if bit {
                SignalState::High
            } else {
                SignalState::Low
            },
        });
        bit_idx += 1;
    }

    // ACK
    t.push(Transition {
        time_ns: bit_start + bit_idx * clock_period_ns,
        state: SignalState::Low,
    });
    bit_idx += 1;

    // Data byte 2
    for &bit in &data2_bits {
        let bt = bit_start + bit_idx * clock_period_ns;
        t.push(Transition {
            time_ns: bt,
            state: if bit {
                SignalState::High
            } else {
                SignalState::Low
            },
        });
        bit_idx += 1;
    }

    // ACK
    t.push(Transition {
        time_ns: bit_start + bit_idx * clock_period_ns,
        state: SignalState::Low,
    });
    bit_idx += 1;

    // STOP: SDA rises while SCL is high
    t.push(Transition {
        time_ns: bit_start + bit_idx * clock_period_ns + half / 2,
        state: SignalState::High,
    });

    t
}

/// Generate decoded I2C fields as Data transitions.
fn build_decoded_transitions(start_ns: u64, clock_period_ns: u64) -> Vec<Transition> {
    let half = clock_period_ns / 2;
    let bit_start = start_ns + half;

    vec![
        Transition {
            time_ns: start_ns,
            state: SignalState::Data("START".to_string()),
        },
        Transition {
            time_ns: bit_start,
            state: SignalState::Data("0x48 W".to_string()),
        },
        Transition {
            time_ns: bit_start + 8 * clock_period_ns,
            state: SignalState::Data("ACK".to_string()),
        },
        Transition {
            time_ns: bit_start + 9 * clock_period_ns,
            state: SignalState::Data("0x0F".to_string()),
        },
        Transition {
            time_ns: bit_start + 17 * clock_period_ns,
            state: SignalState::Data("ACK".to_string()),
        },
        Transition {
            time_ns: bit_start + 18 * clock_period_ns,
            state: SignalState::Data("0x3C".to_string()),
        },
        Transition {
            time_ns: bit_start + 26 * clock_period_ns,
            state: SignalState::Data("ACK".to_string()),
        },
        Transition {
            time_ns: bit_start + 27 * clock_period_ns,
            state: SignalState::Data("STOP".to_string()),
        },
    ]
}
