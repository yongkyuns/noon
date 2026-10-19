# Async, threads, and the deadline

A 5 minute 28 second visual tutorial for reasoning about async, Embassy,
preemptive threads, and real-time control systems. The editable Noon scene is
[async_threads_realtime.py](async_threads_realtime.py).

The lesson follows the same three roles throughout: **C / control** in cyan,
**E / estimation** in amber, and **L / logging** in violet. State labels identify
`READY`, `RUNNING`, and `WAITING` independently of color. It uses a single core so
that concurrency, peripheral progress, and CPU parallelism cannot be confused.

## Run the scene

Open Noon's Python editor and paste the complete `.py` file, then Run. The scene
class is `AsyncThreadsRealtime`. This uses the Noon repository's Python facade
and shared Rust renderer; it does not require Manim or a package from PyPI named
`noon`.

The three settings at the top of the source are the presentation controls:

```python
START_CHAPTER = 1
END_CHAPTER = 13
SPEED = 1.0
```

To inspect the `.await` example alone, set both chapter numbers to `4`. To compare
the three deadline cases, use `5` through `7`. A speed of `0.5` doubles viewing
time; `2.0` halves it. The microsecond values and proportional bar widths remain
unchanged. Chapter time ranges below apply to the complete lesson at `SPEED=1`.

The timeline's pauses are deliberate reading and inspection points. Cursor
movement within each execution interval is linear. The 5 µs dispatch bar is
drawn to scale, even though it is very narrow.

## Chapter guide

| Time | Chapter | What changes on screen |
| --- | --- | --- |
| 0:00–0:12 | Two independent choices | Task state and scheduling policy form separate decisions; Rust ownership is a third axis. |
| 0:12–0:34 | Where suspended state lives | Reserved thread stacks are compared with future state plus an active execution stack. |
| 0:34–0:58 | Waiting releases the CPU | Start DMA, return `Pending`, run other work, wake on completion, then resume. |
| 0:58–1:26 | An await is only a possible yield | A native Rust code display follows the `Ready` path and then the `Pending` path. |
| 1:26–2:00 | Priority cannot interrupt a poll | The controller waits for a long synchronous logger poll and misses its deadline. |
| 2:00–2:25 | Bounded cooperative work can succeed | A bounded poll and selection of control at the next opportunity change the result. |
| 2:25–3:00 | Preemption changes the blocking bound | Higher-priority execution interrupts the logger; the logger later resumes. |
| 3:00–3:28 | Three architectures on one core | One executor, interrupt executors at different priorities, and an RTOS/async hybrid. |
| 3:28–3:50 | What gets faster? | Compare memory, coordination, computation, and deadline response separately. |
| 3:50–4:18 | A mutex can defeat priority | Control waits on the logger's SPI lock while medium-priority estimation runs. |
| 4:18–4:42 | Timeout and cancellation are contracts | A timeout cannot interrupt its own long poll; cancellation can follow external effects. |
| 4:42–5:08 | What can be guaranteed? | Ownership, resource access, timing, and correct physical response need different evidence. |
| 5:08–5:28 | Build the control boundary first | Sensor capture, control, estimation, background work, and monitoring form a bounded architecture. |

## The shared timing example

These are **hypothetical input bounds**, not measured Embassy, RTOS, processor,
or vehicle performance. The diagram isolates one release on one core. Its
purpose is to make a scheduling dependency visible.

| Quantity | Assumption |
| --- | ---: |
| Controller period | 1,000 µs |
| Relative controller deadline | 250 µs |
| Controller CPU execution | 80 µs |
| Release/dispatch overhead used in the illustration | 5 µs |
| Other higher-priority interference | None |
| Controller I/O suspension or other resource waits | None |
| Shared timeline extent | 0–650 µs after release |

Within this deliberately restricted example:

**Response = applicable blocking + dispatch + control execution.**

| Case | Blocking | Dispatch | Control | Completion | Margin to deadline |
| --- | ---: | ---: | ---: | ---: | ---: |
| One long cooperative poll | 500 µs | 5 µs | 80 µs | 585 µs | −335 µs |
| Bounded cooperative poll; C selected next | 50 µs | 5 µs | 80 µs | 135 µs | +115 µs |
| Preemptive scheduling | 10 µs | 5 µs | 80 µs | 95 µs | +155 µs |

The 500 µs logger operation is **synchronous computation**, such as encoding. It
is not a DMA transfer that has correctly suspended its caller. In the bounded
case, every relevant complete poll has a known bound and the controller is
selected at the next opportunity. A fair executor with several ready peers can
add their execution before the controller; the 50 µs bound alone would then be
insufficient.

In the preemptive case, the stated 10 µs covers all applicable lower-priority
nonpreemptible and resource blocking. It is an assumption that needs evidence in
a real design. The later logger continuation is illustrative; only the first
controller job's response is analyzed. The equal 5 µs overhead deliberately
isolates scheduling behavior and establishes no measured advantage for either
an interrupt executor or an RTOS thread.

For a real system, add the effects actually present: higher-priority jobs and
interrupts, release jitter, locks, device transactions, queued work, self
suspension, cache/bus interference, and the actuator path. The familiar
fixed-priority response-time recurrence is useful only when its task model and
blocking assumptions hold. Self-suspending tasks require appropriate analysis,
not simply substituting wall-clock I/O latency for CPU execution. See the
[response-time analysis paper](https://www-users.york.ac.uk/~rd17/papers/ResponseTimeUpperBound3.0.pdf)
and the [self-suspension analysis review](https://link.springer.com/article/10.1007/s11241-018-9316-9).

## What the animation is teaching

### State representation and scheduling are independent

A stackless future preserves the state needed across suspension points. It
still executes on a stack while polled, and locals retained across awaits can be
large. Many waiting activities can therefore need less reserved stack memory
than one conventional stack per activity, but this is conditional on actual
future and stack sizes. The memory picture deliberately uses schematic sizes
and no invented byte counts. Static allocation is available in both styles;
it is not an exclusive async property. See the
[Rust Reference on async blocks](https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks),
[Embassy executor documentation](https://docs.embassy.dev/embassy-executor/0.10.0/cortex-m/index.html),
and [FreeRTOS static allocation documentation](https://www.freertos.org/Documentation/02-Kernel/02-Kernel-features/09-Memory-management/03-Static-vs-Dynamic-memory-allocation).

The separate executor diagrams are intentional. Embassy's Cortex-M interrupt
executor can run async tasks above thread mode, and multiple interrupt
executors can use different interrupt priorities. The hardware scheduling
boundary permits preemption across those priorities. Tasks sharing one executor
still cooperate within their polls. A low-priority RTOS thread can also host an
async executor. Consult the concrete port, executor configuration, and RTOS
policy before applying any one of these diagrams to a product. See
[InterruptExecutor 0.10.0](https://docs.rs/embassy-executor/0.10.0/embassy_executor/struct.InterruptExecutor.html).

### An await is not an unconditional scheduling opportunity

The code shown on screen is an illustrative Rust receive loop, not a complete
driver implementation. When its inner future returns `Ready`, execution can
continue inside the same top-level poll. A queue containing more data can keep
that loop busy. A `Pending` return suspends the computation and gives control
back to the executor. A wakeup makes the task eligible for another poll; it is
not a promise of immediate execution. See the
[Rust await semantics](https://doc.rust-lang.org/reference/expressions/await-expr.html)
and [`Future::poll` contract](https://doc.rust-lang.org/std/future/trait.Future.html).

This is why the visual bounds **a complete poll**, including work reached
through nested async calls. A cooperative design can be appropriate when those
bounds and the executor's selection behavior meet its deadlines. Conversely,
preempting low-priority computation can remove that computation's full poll
length from a higher-priority response bound, while leaving critical sections
and resource blocking relevant. Neither statement establishes a universal
throughput winner. See the
[limited-preemption scheduling survey](https://retis.santannapisa.it/~giorgio/paps/2013/tii2013.pdf).

### Resource and cancellation contracts still matter

An async mutex lets a waiter suspend instead of occupying the CPU, but that
does not inherently provide priority inheritance or a deadline bound. The
priority boost in chapter 10 is explicitly a **generic resource-protocol
illustration**. It is not presented as a feature of an ordinary Embassy async
mutex. A resource owner, dedicated server task, or message-passing design still
creates dependencies that need bounded service and queueing. See
[Embassy Mutex 0.8.0](https://docs.rs/embassy-sync/0.8.0/embassy_sync/mutex/struct.Mutex.html)
and [Zephyr priority inheritance](https://docs.zephyrproject.org/latest/kernel/services/synchronization/mutexes.html).

The timeout picture distinguishes a cooperative wrapper from an independent
enforcement boundary. If a future is executing synchronously inside its poll,
the wrapper cannot check its timeout until that call returns. An independently
schedulable monitor also has platform and interrupt-masking assumptions. A
timeout's existence therefore does not itself establish a CPU budget. See the
[Embassy timer implementation](https://docs.rs/embassy-time/latest/src/embassy_time/timer.rs.html).

Likewise, a losing `select` future can be dropped after a transaction has
started. A driver's memory-safety contract, cancellation behavior, and external
protocol outcome are different questions. Driver-specific cleanup may abort,
wait, or otherwise complete the operation, with corresponding timing costs.
The lesson makes no universal promise of instantaneous DMA cancellation. See
[Embassy select](https://docs.rs/embassy-futures/latest/embassy_futures/select/fn.select.html)
and the [RP DMA driver implementation](https://docs.rs/embassy-rp/latest/src/embassy_rp/dma.rs.html).

### “Safe by construction” needs an explicit property

Safe Rust prevents data races under the language's guarantees and sound API
foundations. General races, deadlocks, missed deadlines, stale input, and wrong
control laws remain distinct problems. These distinctions apply to Rust
threads as well as async tasks. See the
[Rustonomicon's data-race discussion](https://doc.rust-lang.org/nomicon/races.html).

Frameworks can enforce additional resource restrictions. RTIC, for example,
defines task-local and shared resources and a priority-ceiling locking model.
Those are scoped guarantees about that model, not an automatic proof for
arbitrary communication cycles or a complete vehicle. See
[RTIC resources](https://rtic.rs/2/book/en/by-example/resources.html).

The final architecture is a reasoning template. A concrete drone, automotive,
or aerospace system needs requirements for end-to-end latency, input age,
fault detection and containment, and the physical response. Priority expresses
scheduling urgency; criticality and fault containment require their own
decisions. This lesson does not assert certification, WCET qualification, or a
deployable safety architecture.

## Authoring and verification notes

The source uses ordinary Noon `Scene`, native `Text` and Rust-highlighted `Code`,
rectangles, lines, and `FadeIn`/`FadeOut`/leaf transforms. Python tuples collect
authored objects for convenient calls; they do not replace native family
semantics. There are no per-frame callbacks, artificial frame renderers, live
input dependencies, or external artwork. `Scene.time` supplies the logical
clock for chapter-end holds.

The domain arithmetic and proportional axis are checked separately from the
renderer:

```bash
python -m unittest discover -s web/python -p 'test_async_threads_realtime.py'
```

For the hardened supported preview workflow, follow
[`skills/noon-authoring/references/verification.md`](../../../skills/noon-authoring/references/verification.md)
using a current browser build and the pinned Docker runtime. With that setup,
the existing preview entry point accepts this exact source:

```bash
NOON_PREVIEW_RUNTIME_CONFIG=/absolute/path/to/runtime.json \
  node tools/noon-mcp/bin/noon-preview.mjs \
  --source /absolute/path/to/async_threads_realtime.py \
  --output /absolute/path/to/review-output \
  --loop-duration 328 --time 11 --time 85 --time 119 \
  --time 144 --time 179 --time 257 --time 307 --time 327
```

A development capture is a separate category of evidence. Actual Noon canvas
frames captured from the existing `SemanticPreviewSession` host demonstrate
the inspected source, build, times, and backend. A movie encoded from those
frames demonstrates authored-time animation. Neither is automatically a
hardened Docker CLI result, a real-time performance benchmark, or proof of
ordinary Run/replay/seek behavior. Public gallery enrollment also requires the
repository's presentation and playback checks.
