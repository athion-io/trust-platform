use trust_runtime::execution_backend::ExecutionBackend;
use trust_runtime::harness::{bytecode_bytes_from_source, TestHarness};
use trust_runtime::RestartMode;

struct ExecutionBackendFixture {
    name: &'static str,
    source: &'static str,
}

const EXECUTION_BACKEND_CORPUS: &[ExecutionBackendFixture] = &[
    ExecutionBackendFixture {
        name: "call-binding",
        source: r#"
            FUNCTION Add : INT
            VAR_INPUT
                a : INT;
                b : INT := INT#2;
            END_VAR
            Add := a + b;
            END_FUNCTION

            FUNCTION Bump : INT
            VAR_IN_OUT
                x : INT;
            END_VAR
            VAR_INPUT
                inc : INT := INT#1;
            END_VAR
            x := x + inc;
            Bump := x;
            END_FUNCTION

            PROGRAM Main
            VAR
                v : INT := INT#10;
                out_named : INT := INT#0;
                out_default : INT := INT#0;
                out_inout : INT := INT#0;
            END_VAR

            out_named := Add(b := INT#4, a := INT#3);
            out_default := Add(a := INT#3);
            out_inout := Bump(v, INT#5);
            END_PROGRAM
        "#,
    },
    ExecutionBackendFixture {
        name: "string-stdlib",
        source: r#"
            PROGRAM Main
            VAR
                out_left : STRING := '';
                out_mid : STRING := '';
                out_find_found : INT := INT#0;
                out_find_missing : INT := INT#0;
                out_w_replace : WSTRING := "";
                out_w_insert : WSTRING := "";
            END_VAR

            out_left := LEFT(IN := 'ABCDE', L := INT#3);
            out_mid := MID(IN := 'ABCDE', L := INT#2, P := INT#2);
            out_find_found := FIND(IN1 := 'ABCDE', IN2 := 'BC');
            out_find_missing := FIND(IN1 := 'BC', IN2 := 'ABCDE');
            out_w_replace := REPLACE(IN1 := "ABCDE", IN2 := "Z", L := INT#2, P := INT#3);
            out_w_insert := INSERT(IN1 := "ABE", IN2 := "CD", P := INT#3);
            END_PROGRAM
        "#,
    },
    ExecutionBackendFixture {
        name: "refs-sizeof",
        source: r#"
            TYPE
                Inner : STRUCT
                    arr : ARRAY[0..2] OF INT;
                END_STRUCT;
                Outer : STRUCT
                    inner : Inner;
                END_STRUCT;
            END_TYPE

            PROGRAM Main
            VAR
                o : Outer;
                idx : INT := INT#1;
                value_cell : INT := INT#4;
                r_value : REF_TO INT;
                r_outer : REF_TO Outer;
                out_ref : INT := INT#0;
                out_after_write : INT := INT#0;
                out_nested_chain : INT := INT#0;
                out_size_type_int : DINT := DINT#0;
            END_VAR

            r_value := REF(value_cell);
            r_outer := REF(o);
            out_ref := r_value^;
            r_value^ := r_value^ + INT#3;
            out_after_write := r_value^;
            out_nested_chain := r_outer^.inner.arr[idx];
            out_size_type_int := SIZEOF(INT);
            END_PROGRAM
        "#,
    },
];

fn run_execution_backend_bench(workload: ExecutionBackendBenchWorkload) -> anyhow::Result<BenchReport> {
    let mut fixture_reports = Vec::with_capacity(EXECUTION_BACKEND_CORPUS.len());
    let mut aggregate_interpreter_ns = Vec::new();
    let mut aggregate_vm_ns = Vec::new();

    for fixture in EXECUTION_BACKEND_CORPUS {
        let interpreter_samples = collect_backend_samples(
            fixture,
            ExecutionBackend::Interpreter,
            workload.warmup_cycles,
            workload.samples,
        )?;
        let vm_samples = collect_backend_samples(
            fixture,
            ExecutionBackend::BytecodeVm,
            workload.warmup_cycles,
            workload.samples,
        )?;

        aggregate_interpreter_ns.extend(interpreter_samples.iter().copied());
        aggregate_vm_ns.extend(vm_samples.iter().copied());

        fixture_reports.push(build_comparison_summary(
            fixture.name,
            &interpreter_samples,
            &vm_samples,
        ));
    }

    let aggregate = build_comparison_summary("aggregate", &aggregate_interpreter_ns, &aggregate_vm_ns);
    Ok(BenchReport::ExecutionBackend(ExecutionBackendBenchReport {
        scenario: "execution-backend",
        corpus: "mp-060-corpus-v1",
        cycles_per_fixture: workload.samples,
        warmup_cycles: workload.warmup_cycles,
        fixtures: fixture_reports,
        aggregate,
    }))
}

fn collect_backend_samples(
    fixture: &ExecutionBackendFixture,
    backend: ExecutionBackend,
    warmup_cycles: usize,
    samples: usize,
) -> anyhow::Result<Vec<u64>> {
    let mut harness = harness_for_backend(fixture.source, backend)?;
    run_cycles_checked(&mut harness, warmup_cycles, fixture.name, backend, false)?;
    run_cycles_checked(&mut harness, samples, fixture.name, backend, true)
}

fn harness_for_backend(source: &str, backend: ExecutionBackend) -> anyhow::Result<TestHarness> {
    let mut harness = TestHarness::from_source(source).map_err(|err| anyhow::anyhow!("{err}"))?;
    if matches!(backend, ExecutionBackend::BytecodeVm) {
        let bytes = bytecode_bytes_from_source(source).map_err(|err| anyhow::anyhow!("{err}"))?;
        harness
            .runtime_mut()
            .apply_bytecode_bytes(&bytes, None)
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        harness
            .runtime_mut()
            .set_execution_backend(ExecutionBackend::BytecodeVm)
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        harness
            .runtime_mut()
            .restart(RestartMode::Cold)
            .map_err(|err| anyhow::anyhow!("{err}"))?;
    }
    Ok(harness)
}

fn run_cycles_checked(
    harness: &mut TestHarness,
    count: usize,
    fixture_name: &str,
    backend: ExecutionBackend,
    measure: bool,
) -> anyhow::Result<Vec<u64>> {
    let mut samples = if measure {
        Vec::with_capacity(count)
    } else {
        Vec::new()
    };
    for cycle in 0..count {
        let started = Instant::now();
        let result = harness.cycle();
        if !result.errors.is_empty() {
            anyhow::bail!(
                "execution-backend benchmark error (fixture={fixture_name} backend={} cycle={cycle}): {:?}",
                backend_label(backend),
                result.errors
            );
        }
        if measure {
            samples.push(duration_ns(started));
        }
    }
    Ok(samples)
}

fn build_comparison_summary(
    fixture: &'static str,
    interpreter_samples_ns: &[u64],
    vm_samples_ns: &[u64],
) -> ExecutionBackendFixtureReport {
    let interpreter_latency = summarize_ns(interpreter_samples_ns);
    let vm_latency = summarize_ns(vm_samples_ns);
    let interpreter_throughput = throughput_cycles_per_sec(interpreter_samples_ns);
    let vm_throughput = throughput_cycles_per_sec(vm_samples_ns);

    ExecutionBackendFixtureReport {
        fixture,
        interpreter: BackendComparisonSummary {
            latency: interpreter_latency.clone(),
            throughput_cycles_per_sec: interpreter_throughput,
        },
        vm: BackendComparisonSummary {
            latency: vm_latency.clone(),
            throughput_cycles_per_sec: vm_throughput,
        },
        median_latency_ratio: safe_ratio(vm_latency.p50_us, interpreter_latency.p50_us),
        p99_latency_ratio: safe_ratio(vm_latency.p99_us, interpreter_latency.p99_us),
        throughput_ratio: safe_ratio(vm_throughput, interpreter_throughput),
    }
}

fn throughput_cycles_per_sec(samples_ns: &[u64]) -> f64 {
    if samples_ns.is_empty() {
        return 0.0;
    }
    let total_ns: u128 = samples_ns.iter().copied().map(u128::from).sum();
    if total_ns == 0 {
        return 0.0;
    }
    (samples_ns.len() as f64) * 1_000_000_000.0 / (total_ns as f64)
}

fn safe_ratio(lhs: f64, rhs: f64) -> f64 {
    if rhs.abs() <= f64::EPSILON {
        if lhs.abs() <= f64::EPSILON {
            return 1.0;
        }
        return f64::INFINITY;
    }
    lhs / rhs
}

fn backend_label(backend: ExecutionBackend) -> &'static str {
    match backend {
        ExecutionBackend::Interpreter => "interpreter",
        ExecutionBackend::BytecodeVm => "vm",
    }
}
