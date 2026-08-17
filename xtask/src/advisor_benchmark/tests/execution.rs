    #[test]
    fn dedicated_benchmark_rejects_the_recorded_sixteen_gib_capacity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let error = validate_benchmark_capacity(
            &gate,
            13_793_441_254,
            17_179_869_184,
            16 * 1024 * 1024 * 1024,
            15 * 1024 * 1024 * 1024,
            0,
        )
        .expect_err("16 GiB benchmark host must fail");
        assert!(error.to_string().contains("do not run on this host"));
    }
    #[test]
    fn dedicated_benchmark_rejects_existing_swap_pressure() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let error = validate_benchmark_capacity(
            &gate,
            13_793_441_254,
            17_179_869_184,
            64 * 1024 * 1024 * 1024,
            48 * 1024 * 1024 * 1024,
            512 * 1024 * 1024,
        )
        .expect_err("initial swap pressure must fail");
        assert!(error.to_string().contains("swap in use"));
    }

    #[test]
    fn capacity_receipt_reports_every_host_blocker() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        let (_, _, blockers) = benchmark_capacity_blockers(
            &gate,
            13_793_441_254,
            17_179_869_184,
            16 * 1024 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
            512 * 1024 * 1024,
        )
        .expect("capacity evaluation");
        let codes = blockers
            .iter()
            .map(|blocker| blocker.code)
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            [
                "physical_memory_below_required",
                "available_memory_below_required",
                "initial_swap_above_maximum"
            ]
        );
    }

    #[test]
    fn benchmark_child_output_is_drained_but_retained_within_a_fixed_limit() {
        let exceeded = Arc::new(AtomicBool::new(false));
        let result = drain_bounded(
            std::io::Cursor::new(b"abcdefgh"),
            4,
            Arc::clone(&exceeded),
        )
        .expect("bounded drain");
        assert_eq!(result.bytes, b"abcd");
        assert!(result.truncated);
        assert!(exceeded.load(Ordering::Acquire));

        let exceeded = Arc::new(AtomicBool::new(false));
        let result = drain_bounded(
            std::io::Cursor::new(b"abcd"),
            4,
            Arc::clone(&exceeded),
        )
        .expect("exact bounded drain");
        assert_eq!(result.bytes, b"abcd");
        assert!(!result.truncated);
        assert!(!exceeded.load(Ordering::Acquire));
    }

    #[cfg(unix)]
    #[test]
    fn benchmark_child_end_to_end_fault_matrix_is_bounded_and_reaped() {
        struct FaultCase {
            name: &'static str,
            script: &'static str,
            output_limit: usize,
            deadline: Duration,
            expected_termination: Option<&'static str>,
            expected_exit_code: Option<i32>,
        }

        let cases = [
            FaultCase {
                name: "bounded success",
                script: "printf 'ok'",
                output_limit: 4_096,
                deadline: Duration::from_secs(2),
                expected_termination: None,
                expected_exit_code: Some(0),
            },
            FaultCase {
                name: "ordinary child failure",
                script: "printf 'expected failure' >&2; exit 17",
                output_limit: 4_096,
                deadline: Duration::from_secs(2),
                expected_termination: None,
                expected_exit_code: Some(17),
            },
            FaultCase {
                name: "stalled child deadline",
                script: "while :; do :; done",
                output_limit: 4_096,
                deadline: Duration::from_millis(75),
                expected_termination: Some("benchmark_child_deadline"),
                expected_exit_code: None,
            },
            FaultCase {
                name: "term-resistant child deadline",
                script: "trap '' TERM; while :; do :; done",
                output_limit: 4_096,
                deadline: Duration::from_millis(75),
                expected_termination: Some("benchmark_child_deadline"),
                expected_exit_code: None,
            },
            FaultCase {
                name: "oversized stdout",
                script: "while :; do printf '0123456789abcdef'; done",
                output_limit: 1_024,
                deadline: Duration::from_secs(2),
                expected_termination: Some("benchmark_child_output_limit"),
                expected_exit_code: None,
            },
            FaultCase {
                name: "oversized stderr",
                script: "while :; do printf '0123456789abcdef' >&2; done",
                output_limit: 1_024,
                deadline: Duration::from_secs(2),
                expected_termination: Some("benchmark_child_output_limit"),
                expected_exit_code: None,
            },
        ];
        let temporary = tempfile::tempdir().unwrap();
        let watchdog = BenchmarkWatchdog {
            minimum_available_memory_bytes: 0,
            maximum_swap_growth_bytes: u64::MAX,
            initial_swap_used_bytes: swap_used_bytes().unwrap_or(0),
        };

        for case in cases {
            let started = Instant::now();
            let output = timed_output_with_limits(
                Path::new("/bin/sh"),
                &["-c".to_string(), case.script.to_string()],
                temporary.path(),
                watchdog,
                ChildExecutionLimits {
                    output_limit_bytes: case.output_limit,
                    deadline: case.deadline,
                    poll_interval: Duration::from_millis(10),
                    resource_monitor_stall_deadline: None,
                    require_resource_measurements: false,
                },
            )
            .unwrap_or_else(|error| panic!("{} failed: {error:#}", case.name));

            assert_eq!(
                output.termination_reason, case.expected_termination,
                "{} termination",
                case.name
            );
            assert_eq!(
                output.output.status.code(),
                case.expected_exit_code,
                "{} exit status",
                case.name
            );
            assert!(
                output.output.stdout.len() <= case.output_limit,
                "{} retained too much stdout",
                case.name
            );
            assert!(
                output.output.stderr.len() <= case.output_limit + 256,
                "{} retained too much stderr",
                case.name
            );
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "{} was not terminated and reaped promptly",
                case.name
            );
        }
    }

    #[test]
    fn benchmark_gate_cannot_weaken_the_fixed_runtime_floor() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut gate: BenchmarkReleaseGate = serde_json::from_slice(
            &fs::read(root.join("benchmarks/advisor/release-gate.json")).unwrap(),
        )
        .unwrap();
        gate.minimum_available_memory_reserve_bytes = 4 * 1024 * 1024 * 1024;
        assert!(validate_benchmark_gate(&gate).is_err());
    }
