# Async, threads, and the deadline

A visual tutorial about **how computation runs, waits, and resumes** under
threaded and async execution. The editable Noon scene is
[async_threads_realtime.py](async_threads_realtime.py). Its current cue sheet
contains thirteen chapters and 6 minutes 48 seconds of authored time.

The central comparison has two independent questions: **where is unfinished
work saved, and when may another computation take the CPU?** Async can reduce
memory and coordination costs for a particular workload. Preemption can reduce
the delay imposed by lower-priority computation. Neither choice, by itself,
establishes the resource, timing, or physical-system guarantees needed by a
drone, vehicle, or aircraft. The lesson makes those dependencies visible before
discussing particular frameworks.

## Read the visual language

The same three roles recur throughout the lesson: **C / control** in cyan,
**E / estimation** in amber, and **L / logging** in violet. Letters and state
labels carry the meaning independently of color.

| Visual | Meaning | How to interpret its movement |
| --- | --- | --- |
| Large tile marked C, E, or L | One computation and its saved execution position | Moving between stations means a state or scheduling transition. It does not imply copying all task memory. |
| Four small marks inside a task tile | Qualitative completed work | Lit marks travel with the task. They stop changing while the task is outside the CPU. They are not percentages, equal instruction counts, or a time scale. |
| `READY` station | Eligible work that has not yet been selected | A wakeup can move a task here without giving it the CPU immediately. |
| `WAITING` station | Work that cannot continue until an event or resource becomes available | The saved computation remains intact while other work runs. |
| `PAUSED` or `PREEMPTED` station | A ready computation displaced by higher-priority execution | This is a visual annotation for preemption, not a claim that the task is blocked on I/O. |
| CPU station | The selected computation | Only the task executing here accumulates CPU work. |
| Small traveling square | Data or an event | A sample, queued packet, completion notification, or command can travel independently of a task tile. |
| Peripheral progress track | Device work, such as DMA | Its progress can overlap CPU work without introducing a second CPU. |
| Growing timeline and cursor | The hypothetical execution intervals in chapters 5–7 | Read the microsecond axis for timing; the four task marks remain schematic. |

Connectors are drawn progressively along explicit horizontal and vertical
routes. Labels are revealed as each relationship is introduced. A marked
junction is a deliberate scheduling merge, not an accidental crossing or an
additional processor.

There is **one core per illustrated design**. Chapter 2 places two alternative
memory organizations side by side; they are separate examples, not a dual-core
machine. The final architecture shows functional blocks, so a task tile in a
block identifies that block's currently illustrated computation. It does not
mean every block has its own CPU.

## Run the scene

Open Noon's Python editor and paste the complete `.py` file, then Run. The scene
class is `AsyncThreadsRealtime`. This uses the Noon repository's Python facade
and shared engine; it does not require Manim or a package from PyPI named
`noon`.

The three settings at the top of the source are the presentation controls:

```python
START_CHAPTER = 1
END_CHAPTER = 13
SPEED = 1.0
```

Set both chapter numbers to `3` to compare the two I/O walkthroughs, or to `4`
to follow a busy async task consuming available packets. Use `5` through `7` to
compare the three deadline cases. A speed of `0.5` doubles viewing time; `2.0`
halves it. The illustrated microsecond values and proportional bar widths remain
unchanged.

The scene contains reading holds and enlarged scheduling transitions. Movie
seconds are therefore **presentation time**, not a measurement of processor
time. During each numerical execution interval the cursor and bar advance
linearly; the 5 µs dispatch interval remains narrow on the common axis. Follow
that axis, rather than the wall-clock duration of the animation, when comparing
response times.

## Chapter guide

These times follow the source cue sheet at `SPEED=1` and the thirteen chapter
markers in the verified 408-second development export.

| Time | Chapter | What to watch |
| --- | --- | --- |
| 0:00–0:16 | State and scheduling are separate choices | A two-by-two matrix separates thread/async state representation from cooperative/preemptive scheduling. |
| 0:16–0:42 | Paused work keeps its place | Task tiles leave the CPU, preserve their work marks, and resume from saved stacks or continuation state. |
| 0:42–1:24 | Both runtimes can release the CPU for I/O | The same transfer runs first with a blocking thread, then with an async task; L computes while C waits and DMA progresses. |
| 1:24–1:56 | Available work can keep the CPU busy | Packets arrive from a queue; L keeps computing until an unavailable operation makes it suspend. |
| 1:56–2:30 | Selection priority cannot interrupt a poll | C remains ready while a long uninterrupted L computation grows past C's deadline. |
| 2:30–2:58 | Bounded cooperative work can succeed | L returns control earlier; selecting C next produces a different response bound. |
| 2:58–3:36 | Preempt, preserve, resume | L stops with its marks intact, C executes, and the original L computation resumes. |
| 3:36–4:12 | Three architectures, still one CPU | One executor, separate preemptive priorities, and an RTOS/async hybrid select work through the same CPU boundary. |
| 4:12–4:36 | Which performance metric improves? | Compare RAM, coordination, computation, and deadline response separately. |
| 4:36–5:18 | The highest priority can still be blocked | The SPI owner and running computation diverge; a resource protocol lets the owner make the progress C needs. |
| 5:18–5:48 | Timeout and cancellation need contracts | A timer becomes ready during busy computation; a later cancellation leaves an already-issued external command to be handled. |
| 5:48–6:12 | Safety needs explicit guarantees | Memory, resources, timing, and correct physical response require different evidence. |
| 6:12–6:48 | Follow the sample all the way to actuation | Sample, snapshot, command, log, and monitor pulses follow a six-block architecture with separate routes. |

## 1. Separate saved state from CPU scheduling

The opening matrix prevents a common mismatch: comparing a cooperative async
executor with a preemptive RTOS and attributing every difference to async versus
threads. A conventional thread has a saved execution context and stack. A
stackless async task saves a continuation and the data required to resume. The
choice of scheduler determines when one execution context can displace another.
Concrete systems provide combinations of these properties: Zephyr documents
both cooperative and preemptible threads, while Embassy provides interrupt
executors that can run async tasks at different hardware priorities. See
[Zephyr thread priorities](https://docs.zephyrproject.org/latest/kernel/services/threads/index.html#thread-priorities)
and [Embassy InterruptExecutor 0.10.0](https://docs.rs/embassy-executor/0.10.0/embassy_executor/struct.InterruptExecutor.html).

In chapter 2, watch the completed mark on C before it leaves the CPU. It remains
lit while L runs, and C continues from that position when selected again. Both
models preserve unfinished computation. The memory difference concerns how
much storage is reserved and what must remain live during suspension.

For the stackless async design shown, many waiting tasks can share an active
execution stack instead of each reserving a conventional thread stack. Each
task still needs retained state, and the currently executing task still uses a
stack. Large retained buffers, nested calls, and multiple preemptive execution
contexts must be counted. The boxes are schematic: they establish no byte
ratio. Static allocation is possible in both styles. See the
[Embassy executor documentation](https://docs.embassy.dev/embassy-executor/0.10.0/cortex-m/index.html)
and [FreeRTOS static allocation documentation](https://www.freertos.org/Documentation/02-Kernel/02-Kernel-features/09-Memory-management/03-Static-vs-Dynamic-memory-allocation).

The useful comparison is therefore the complete memory budget: task state,
stacks, queues, retained buffers, and runtime bookkeeping for the same required
workload. The word “async” alone does not determine that total.

The lesson isolates one CPU to make scheduling visible. With several cores,
several computations can execute at once if the runtime and workload permit
it. A multithreaded async runtime combines async tasks with thread scheduling;
async and threads are still compatible choices. Parallelism comes from available
execution resources and independent work. More cores also expand the timing
argument to include shared-memory, cache, and bus interference.

## 2. Follow a complete wait and wake cycle

Chapter 3 deliberately performs the **same sequence twice**. First C is a
blocking thread; then it is an async task. The data source, CPU, and logger do
not change between passes.

1. **Run and request.** C enters the CPU, completes some work, and starts the
   sensor transfer. A small square follows the device-request route.
2. **Wait.** C leaves the CPU with its completed mark intact. The blocking thread
   retains its call stack; the async task retains its continuation state.
3. **Overlap useful work.** L enters the CPU and its marks advance. The DMA
   indicator moves on a separate track. C's marks remain frozen.
4. **Become ready.** Device completion makes C eligible to continue. Its tile
   travels to READY and pauses there. Readiness has changed; CPU ownership has
   not yet changed.
5. **Resume when selected.** L leaves the CPU and C returns, continuing from its
   saved position.

The insight is that waiting for I/O need not occupy the CPU in either model.
The threaded example uses a blocking wait that deschedules its caller, not a
busy loop. The async example uses a wait that actually suspends. A blocking
driver called from a cooperative executor could instead hold up the executor;
that would be a different execution contract. The distinction between ready,
running, and waiting is visible in the
[Zephyr thread-state documentation](https://docs.zephyrproject.org/latest/kernel/services/threads/index.html#thread-states)
and the async
[`Future::poll` contract](https://doc.rust-lang.org/std/future/trait.Future.html).

## 3. An async operation need not hand over the CPU

In chapter 4, L consumes several already-available packets while C stays in
READY. Each packet is small, but the sequence remains one uninterrupted period
of CPU occupation. Only after the queue is empty does L leave for WAITING and
allow another task to execute.

The visual mechanism is independent of source syntax: **an operation that can
complete immediately may let its caller continue immediately**. In the
poll-based runtime used as the lesson's concrete async example, an inner
operation returning ready does not require the outer task to return to its
executor. Repeated available work can keep that outer poll running. A return
indicating that the task is pending gives scheduling control back. See the
[await semantics](https://doc.rust-lang.org/reference/expressions/await-expr.html)
and [`Future::poll` contract](https://doc.rust-lang.org/std/future/trait.Future.html).

Consequently, a cooperative response-time argument must bound the **entire
uninterrupted execution segment**, including nested calls and repeated
immediately-ready operations. Counting awaits or putting a high-priority label
on C does not establish that bound. Some runtimes enforce additional fairness
or work budgets; the relevant question is what the selected runtime actually
guarantees.

## 4. Compare three scheduling outcomes on the same axis

Chapters 5–7 use **hypothetical input bounds**, not measured Embassy, RTOS,
processor, or vehicle performance. They isolate one controller release on one
core so that a single scheduling dependency is easy to see.

| Quantity | Assumption |
| --- | ---: |
| Controller period | 1,000 µs |
| Relative controller deadline | 250 µs |
| Controller CPU execution | 80 µs |
| Release/dispatch overhead in the illustration | 5 µs |
| Other higher-priority interference | None |
| Controller I/O suspension or additional resource waits | None |
| Shared timeline extent | 0–650 µs after release |

Within this restricted model:

**Response = applicable blocking + dispatch + control execution.**

| Case | Blocking | Dispatch | Control | Completion | Margin to deadline |
| --- | ---: | ---: | ---: | ---: | ---: |
| One long cooperative poll | 500 µs | 5 µs | 80 µs | 585 µs | −335 µs |
| Bounded cooperative poll; C selected next | 50 µs | 5 µs | 80 µs | 135 µs | +115 µs |
| Preemptive scheduling | 10 µs | 5 µs | 80 µs | 95 µs | +155 µs |

### Long cooperative execution: C finishes at 585 µs

L is doing synchronous computation, such as encoding. It occupies the CPU for
500 µs after C's release. C is ready throughout, but the executor cannot choose
it while that poll is still running. The growing violet interval passes the
250 µs deadline before C begins. Dispatch takes the stated 5 µs, then C performs
the same 80 µs of work used in every case.

The delay is not a DMA transfer that has correctly suspended its caller. That
case was shown separately in chapter 3.

### Bounded cooperation: C finishes at 135 µs

L returns control after at most 50 µs, and this example explicitly assumes
**C is selected next**. C can therefore start after 55 µs and finish after
135 µs. The control algorithm did not become faster; its opportunity to execute
arrived sooner.

A queue of ready peers could add their execution before C. Thus a 50 µs bound
on an individual poll is insufficient unless the selection and queueing
assumptions also support the result. More frequent yields can improve access to
the CPU while adding coordination work. Both effects belong in the design.

### Preemption: C finishes at 95 µs and L resumes

Keep L's long computation, but permit a higher-priority execution boundary to
preempt it. In the drawn trace, L first executes a 10 µs nonpreemptible segment.
Its tile then moves to the preempted station without finishing. C runs after
the 5 µs dispatch interval and completes at 95 µs. L subsequently returns with
the same marks and continues its remaining computation.

Using 10 µs as the blocking term assumes that **all applicable lower-priority
blocking is covered by that bound**. Additional locks or masked interrupts
would require evidence and could change it. The later L continuation is a
visual demonstration of preservation and resumption; only the first C job's
response is analyzed. Equal 5 µs dispatch costs deliberately isolate the
scheduling effect and claim no measured advantage for either runtime.

Preemption can remove a lower-priority task's entire uninterrupted computation
from the higher-priority blocking term while leaving nonpreemptible sections
relevant. This is a scheduling property, available to appropriately configured
threads or priority-separated async execution. See the
[limited-preemption scheduling survey](https://retis.santannapisa.it/~giorgio/paps/2013/tii2013.pdf).

A real response analysis must also account for the effects actually present:
higher-priority jobs and interrupts, release jitter, resource dependencies,
device transactions, queued work, self-suspension, and cache or bus interference.
Fixed-priority response-time analysis is useful only when its task model and
blocking assumptions hold. Self-suspending tasks require suitable analysis;
wall-clock I/O latency is not simply interchangeable with CPU execution. See
the [response-time analysis paper](https://www-users.york.ac.uk/~rd17/papers/ResponseTimeUpperBound3.0.pdf)
and the [self-suspension analysis review](https://link.springer.com/article/10.1007/s11241-018-9316-9).

## 5. Put priorities at an execution boundary

Chapter 8 retains one CPU while changing the containers around C, E, and L.
The marked scheduling junction selects one computation at a time.

| Architecture | Where another computation can take over | Consequence for C |
| --- | --- | --- |
| One cooperative executor | The running task returns scheduling control | C depends on the uninterrupted work and selection behavior inside that executor. |
| Separate preemptive priorities | A higher-priority execution context can interrupt a lower one, subject to platform restrictions | C can run before the lower-priority computation reaches its normal suspension point. |
| RTOS threads with background async | The RTOS schedules threads; the background executor schedules its own tasks | Control can have a preemptive thread boundary while waiting-heavy activities share an async runtime. |

Embassy's Cortex-M interrupt executor is one practical example of the middle
row. Multiple interrupt executors can use different interrupt priorities, so
hardware preemption separates those execution contexts. Tasks sharing a single
executor still cooperate within their polls. Consult the concrete port and
configuration before treating an executor label as a preemption guarantee. See
[InterruptExecutor 0.10.0](https://docs.rs/embassy-executor/0.10.0/embassy_executor/struct.InterruptExecutor.html).

The hybrid row also makes an important design possibility visible: threaded and
async execution can be combined. The architecture question is where to place
the boundaries that the timing and resource arguments require.

## 6. Say which kind of performance improves

The scene does not assign a universal winner. It separates costs that are often
collapsed into the word “faster.”

| Metric | Potential benefit to investigate | Cost or condition that remains |
| --- | --- | --- |
| Memory | Stackless state can reduce reserved storage for many waiting activities. | Count retained buffers, actual state sizes, execution stacks, and queues for both designs. |
| Coordination | A runtime may implement wait, wake, and task selection with less work for a particular workload. | Measure the chosen executor, RTOS, driver, and synchronization paths; yields and polling also consume CPU. |
| Computation throughput | Reducing coordination overhead can leave more CPU time for useful work. | Async syntax does not remove the estimator or controller's arithmetic. |
| Deadline response | Earlier suspension or preemption can make urgent work start sooner. | Bound blocking, selection, interference, and the complete actuator path. |
| Energy and jitter | Different wake and scheduling behavior can change these outcomes. | Measure each outcome under the same workload and timing obligations. |

For a fair comparison, give both implementations the same useful work,
equivalent I/O support, allocation constraints, priorities, and required
guarantees. A design that wins average throughput may still fail a particular
worst-case deadline. The memory mechanisms above and the scheduling analysis
in chapters 5–8 explain why both an improvement and a regression are possible;
the animation supplies no platform benchmark.

## 7. Trace the resource dependency, not just the priority label

Chapter 10 gives SPI a separate **OWNER: L** label. Resource ownership and CPU
ownership are different facts.

1. C executes but cannot obtain the SPI resource already owned by L.
2. C waits. Medium-priority E runs, while the lower-priority owner L makes no
   progress toward releasing the resource.
3. A generic priority-inheritance protocol raises L's effective priority so it
   can finish the protected work that is holding C up.
4. L releases SPI; C becomes ready, is selected, and obtains the resource.

The distinction survives an async rewrite. Suspending a mutex waiter avoids
occupying the CPU, but does not inherently provide priority inheritance or a
bounded wait. The illustrated boost is a **generic resource-protocol example**,
not a claim about an ordinary Embassy async mutex. See
[Embassy Mutex 0.8.0](https://docs.rs/embassy-sync/0.8.0/embassy_sync/mutex/struct.Mutex.html)
and [Zephyr priority inheritance](https://docs.zephyrproject.org/latest/kernel/services/synchronization/mutexes.html).

A dedicated resource owner or message-passing server changes how the dependency
is expressed; its service time and queue can still delay C. Bound lock hold
times, service queues, and device transactions as well as computation. The
priority label is only useful together with a resource protocol that supports
the desired bound.

## 8. Separate timer readiness, enforcement, and cancellation

The first half of chapter 11 shows a timeout event becoming ready while the
wrapped computation is still busy. The event does not become another task that
magically steals the CPU. A cooperative wrapper cannot check its timer while
an inner synchronous call has not returned. A separate monitor needs an
independently schedulable enforcement boundary, and that boundary still has
interrupt-masking and platform assumptions. See the
[Embassy timer implementation](https://docs.rs/embassy-time/latest/src/embassy_time/timer.rs.html).

This separates two contracts: “the operation reports a timeout when it can
observe the timer” and “execution cannot consume more than a stated budget.”
The first does not establish the second.

In the second half, follow a command from CALLER through DRIVER to DEVICE.
Cancellation happens after the command has been issued. Stopping the caller's
continuation does not rewind the device. The driver and protocol must define
whether the operation aborts, finishes, or requires deferred cleanup, including
buffer lifetime and timing obligations. Thread termination needs analogous
resource handling; the external effect is not determined solely by how the
calling computation was represented. The concrete async example is documented
by [Embassy select](https://docs.rs/embassy-futures/latest/embassy_futures/select/fn.select.html)
and the [RP DMA driver implementation](https://docs.rs/embassy-rp/latest/src/embassy_rp/dma.rs.html).

## 9. Define the property meant by “safe by construction”

Chapter 12 turns the phrase into four separate claims. The table is a set of
questions for the proposed design, not a claim that either runtime supplies
every guarantee.

| Layer | Property to state | Evidence the design must provide |
| --- | --- | --- |
| Memory | Which accesses, lifetimes, and buffer transfers are valid? | Enforced language/API rules, correct driver boundaries, and any required isolation. |
| Resources | Who may own or wait for each resource, and for how long? | An enforced resource protocol with stated assumptions about queues and devices. |
| Timing | Will each required result arrive within its bound? | Execution bounds, blocking and interference analysis, and suitable enforcement of assumptions. |
| System | Is the timely physical response the required response? | Correct control requirements, data-age rules, fault handling, and verification of the complete path. |

Removing arbitrary preemption inside an execution context can simplify some
local reasoning, but shared resources, suspension points, interrupts, and
external devices still define concurrency boundaries. A memory-safe program
can have a deadlock or a missed deadline. A timely program can use stale input
or apply the wrong control law. A safe-by-construction argument should name the
property and the assumptions that make it hold. Concrete language and resource
examples appear in the optional implementation notes below.

## 10. Follow the sample all the way to actuation

The last chapter arranges six blocks in two rows. The short upper routes carry
the sensor sample and actuator command. The estimator's snapshot travels up a
separate route. Logging and monitoring have their own paths so their roles can
be followed without crossing unrelated arrows.

1. **Capture a sample.** The sample pulse enters the control path. Its timestamp
   matters as well as its arrival.
2. **Publish state.** Estimation completes the illustrated work and publishes a
   coherent snapshot. Control needs a specified maximum age and consistency
   contract for the snapshot it consumes.
3. **Compute control.** C accumulates work, then emits a command. The scheduling
   boundary must support the control response bound.
4. **Apply the command.** The command reaches the actuator. End-to-end latency
   includes this path; finishing the control calculation alone is insufficient.
5. **Queue background work.** A log pulse enters a bounded queue. The design
   needs an explicit overflow and backpressure policy at the control boundary.
6. **Detect and contain faults.** The monitor has a distinct fault path and
   stated budget. Its ability to act requires the platform and independence
   assumptions represented by that boundary.

This is a reasoning template for deciding where threads, async tasks, or a
hybrid belong. It asks for end-to-end timing, input age, resource access,
overload behavior, and fault-response contracts. Priority expresses scheduling
urgency; criticality and fault containment require additional design decisions.
The tutorial does not supply a certified implementation or a qualified WCET
argument for a particular vehicle.

## Optional implementation notes

The animation's focus is execution, not language syntax. These primary sources
connect the concepts to specific implementations for readers who want that
detail.

- **Stackless async state and ready/pending behavior:** the
  [Rust Reference on async blocks](https://doc.rust-lang.org/reference/expressions/block-expr.html#async-blocks),
  [await semantics](https://doc.rust-lang.org/reference/expressions/await-expr.html),
  and [`Future::poll`](https://doc.rust-lang.org/std/future/trait.Future.html)
  explain the example runtime mechanism. These are concrete semantics, not
  universal requirements on every language that uses the word async.
- **Memory safety versus other races:** the
  [Rustonomicon's data-race discussion](https://doc.rust-lang.org/nomicon/races.html)
  distinguishes data races from broader race conditions. Safe Rust's guarantees
  depend on sound language and API foundations and apply to Rust threads as
  well as async tasks. They do not themselves prove deadlines or control laws.
- **Enforced resource rules:**
  [RTIC resources](https://rtic.rs/2/book/en/by-example/resources.html) documents
  task-local and shared resources and a priority-ceiling locking model. Such
  guarantees are scoped to the enforced model; arbitrary communication cycles
  and a complete physical system need their own argument.

## Authoring and verification notes

The source uses ordinary Noon `Scene`, native `Text`, `VGroup` task tiles,
rectangles, lines, and path objects. Progressive path and arrow reveals,
written labels, fades, and native transforms express the visual transitions.
Python tuples collect authored objects for convenient calls; they do not
replace native family semantics. There are no per-frame callbacks, artificial
frame renderers, live input dependencies, or external artwork. `Scene.time`
supplies the logical clock for chapter-end holds.

**Revised development export verified.** The exact-source movie contains
12,240 frames at 30 fps, 1600×900, with thirteen chapter markers and a duration
of 408 seconds. The source reaches its completed state at exactly 408 seconds.
All 12,651 native observations—including pre-roll and separate range
endpoints—matched their requested sample times. No browser or renderer errors
were reported. Every duration-bearing frame index from 0 through 12,239 occurs
exactly once, in order.

The independently rendered ranges join at 136 and 272 seconds with exact
decoded RGBA equality. The assembled MP4 passed a complete decode with no error
output. The 29 retained chapter images match their recorded native PNG hashes.
Chapter layouts, caption transitions, task preservation/resumption, and
representative exact-source frames were visually inspected, including the final
architecture at 407 seconds. A same-source 21-second execution excerpt also
passed complete decode and motion inspection at 30 fps.

The scene source SHA-256 is
`eba9cda2e64f212d414386f0bb22be7978607982ec8b4239b589812139321f6f`.
The actual Noon WebGL2 engine build matches repository base commit
`b11f450551ef2db4daf99f873b16cab974e4c887`, with build ID
`d6cd1c9a0dc2bbc16657db8003ebea4971c5b89ebb122b2089e2579ad69b19ab`.
The image archive includes capture provenance and movie verification metadata.

The four focused tests for domain arithmetic, the exact-deadline boundary,
the proportional axis, and the complete cue sheet passed separately from the
renderer:

```bash
python3 -m unittest web/python/test_async_threads_realtime.py
```

The repository entry point `bash scripts/check.sh fast` was attempted but
stopped at its architecture gate because Cargo is unavailable in this local
environment. The focused tests and development export do not replace that gate.

For the hardened supported preview workflow, follow
[`skills/noon-authoring/references/verification.md`](../../../skills/noon-authoring/references/verification.md)
using a current browser build and the pinned Docker runtime. With that setup,
the preview entry point accepts this source and the current authored duration:

```bash
NOON_PREVIEW_RUNTIME_CONFIG=/absolute/path/to/runtime.json \
  node tools/noon-mcp/bin/noon-preview.mjs \
  --source /absolute/path/to/async_threads_realtime.py \
  --output /absolute/path/to/review-output \
  --loop-duration 408 --time 15 --time 83 --time 115 \
  --time 149 --time 177 --time 215 --time 317 --time 407
```

A development capture is a separate category of evidence. Actual Noon canvas
frames captured from the existing `SemanticPreviewSession` host demonstrate
the inspected source, build, times, and backend. A movie encoded from those
frames demonstrates authored-time animation. Neither is automatically a
hardened Docker CLI result, a real-time performance benchmark, or proof of
ordinary Run/replay/seek behavior. Public gallery enrollment also requires the
repository's presentation and playback checks.
