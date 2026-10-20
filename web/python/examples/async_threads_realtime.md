# Async, threads, and the deadline

A visual tutorial about **how computation runs, waits, and resumes** under
threaded and async execution. The editable Noon scene is
[async_threads_realtime.py](async_threads_realtime.py). This revision connects
three recurring, language-neutral run loops with the runtime that selects their
work. Explanations introduce each event before it moves; deliberate holds give
the resulting state time to be inspected. The authored cue sheet schedules
thirteen chapters over **8 minutes 42 seconds** at `SPEED=1`.

The central comparison has two independent questions: **where is unfinished
work saved, and when may another computation take the CPU?** Async can reduce
memory and coordination costs for a particular workload. Preemption can reduce
the delay imposed by lower-priority computation. Neither choice, by itself,
establishes the resource, timing, or physical-system guarantees needed by a
drone, vehicle, or aircraft. The lesson makes those dependencies visible before
discussing particular frameworks.

## Read the visual language

The same three roles recur throughout the lesson: **C / control** in cyan,
**E / estimation** in amber, and **L / logging** in violet. Each has a recurring
pseudocode panel and a visible execution marker. Letters, operation names, and
state labels carry the meaning independently of color.

| Visual | Meaning | How to interpret its movement |
| --- | --- | --- |
| Stationary program card marked C, E, or L | The recurring program performed by that role | Its position stays stable while selection, state, and execution change. |
| Tiny glowing pointer beside an operation | The program's current illustrated position | Only the CPU-selected loop can advance. Ready, waiting, and preempted loops preserve their position. |
| Small work marks on a task | Qualitative completed work | They can advance within one selected operation while its pointer stays on that line. They are not percentages, equal instruction counts, or a time scale. |
| `READY` label inside a program card | Eligible work that has not yet been selected | A wakeup changes readiness; the saved pointer waits for CPU selection. |
| `WAITING` label inside a program card | Work that cannot continue until an event or resource becomes available | The card and saved pointer stay in place while another program runs. |
| `PREEMPTED` label inside a program card | A ready computation displaced by higher-priority execution | Its operation and partial progress are preserved. This does not mean it is blocked on I/O. |
| CPU selection indicator | The selected computation | Only the selected task accumulates CPU work. Its pointer moves when the illustrated operation advances. |
| Larger task token marked C, E, or L | The compact computation view used in the memory and final-architecture chapters | Saved work persists across context changes; a token inside a functional block does not give that block its own CPU. |
| Small glowing transport circle | Data or an event | A sample, queued packet, completion notification, or command can travel independently of a task. The circle's motion is not CPU execution. |
| Brief focus outline | The subject named by the current explanation | The outline directs attention to an operation, queue, resource, or route. It is separate from the glowing transport circles and execution pointers. |
| Peripheral progress track | Device work, such as DMA | Its progress can overlap CPU work without introducing a second CPU. |
| Growing timeline and cursor | The hypothetical execution intervals in chapters 5–7 | Read the microsecond axis for timing; task-work marks and pseudocode positions remain schematic. |

Connectors are drawn progressively along explicit horizontal and vertical
routes. A traveling circle follows an already-introduced route. Focus makes the
current cause or consequence stand out while the rest of the architecture
remains available for context. In the execution chapters, the program cards
stay in place: CPU selection and state labels change around the saved programs.

Each teaching beat has three stages:

1. **EXPLAIN.** Read what is about to happen and identify the highlighted
   operation or runtime region.
2. **WATCH.** Follow one cause and its consequence: a task is
   selected, a computation progresses, data arrives, or an operation blocks.
3. **NOTICE.** Use the hold to compare the resulting CPU owner, task state,
   execution marker, and resource state before the next event begins.

There is **one core per illustrated design**. Chapter 2 places two alternative
memory organizations side by side; they are separate examples, not a dual-core
machine. The final architecture shows functional blocks, so a task token in a
block identifies that block's currently illustrated computation. It does not
mean every block has its own CPU.

## The three recurring run loops

The panels use pseudocode to show purpose and program position. They are not
Rust listings or complete driver implementations. The same operations recur
when the lesson changes the surrounding runtime.

The three stationary cards are introduced in chapter 1 and recur in chapters
3–8, chapter 10, and the first part of chapter 11. The memory and final
architecture chapters use compact task tokens. Each example may begin partway
through a loop; its pointer identifies the current operation at that point.

### C — control

```text
repeat
  wait for release
  acquire sample
  read state snapshot
  compute command
  send command
```

### E — estimation

```text
repeat
  wait for measurement
  update estimate
  publish snapshot
```

### L — logging

```text
repeat
  receive packet
  encode + buffer
  if due: flush output
```

“Flush if due” makes it possible for L to receive and encode several available
packets before it needs to wait. The resource chapter expands that output step
to show shared SPI ownership. C's acquire step similarly expands into request,
wait, and completion in the I/O chapter.

**Only the CPU-selected loop advances its execution marker.** If C blocks in
acquire, its marker stays there while L computes. A completion event may make C
ready, but the marker still waits for C to be selected. If L is preempted during
encoding, its marker preserves that operation and resumes there later.

A pseudocode line can represent a long computation or a multi-stage driver
operation. It is not an atomic instruction or a fixed amount of time.
Preemption can stop a task partway through the highlighted line; a saved marker
and the work indicator together make that partial progress visible.

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

These timestamps match the verified revision 3 development export at
`SPEED=1`: 522 seconds, 15,660 frames at 30 fps, and 13 chapter markers.
The authoring and verification notes below describe the captured evidence and
its scope.

| Time | Chapter | What to watch |
| --- | --- | --- |
| 0:00–0:32 | 1. State and scheduling are separate choices | Three recurring programs and two choice cards separate state representation from scheduling. |
| 0:32–1:06 | 2. Paused work keeps its place | Task tokens preserve completed work as contexts are saved and resumed. |
| 1:06–1:58 | 3. Both runtimes can release the CPU for I/O | C's acquire pointer stays still while L computes and device completion changes C's readiness. |
| 1:58–2:38 | 4. Available work can keep the CPU busy | L's marker cycles through available receives and encoding while C's ready pointer stays still. |
| 2:38–3:18 | 5. Selection priority cannot interrupt a poll | The focus stays on L's uninterrupted computation as its timeline grows past C's deadline. |
| 3:18–3:52 | 6. Bounded cooperative work can succeed | L returns control earlier; selecting C next produces a smaller response bound. |
| 3:52–4:36 | 7. Preempt, preserve, resume | C finishes at 95 µs; L then resumes with qualitative work and its saved operation intact. |
| 4:36–5:18 | 8. Three architectures, still one CPU | One executor, separate preemptive priorities, and an RTOS/async hybrid select the same recurring programs. |
| 5:18–5:48 | 9. Which performance metric improves? | A focused comparison separates RAM, coordination, computation, and deadline response. |
| 5:48–6:34 | 10. The highest priority can still be blocked | SPI ownership, C's blocked operation, and the CPU-selected loop are different facts. |
| 6:34–7:16 | 11. Timeout and cancellation need contracts | A timer becomes ready during busy computation; an issued command remains an external effect after cancellation. |
| 7:16–7:48 | 12. Safety needs explicit guarantees | A stale-snapshot example precedes the memory, resource, timing, and system guarantee layers. |
| 7:48–8:42 | 13. Follow the sample all the way to actuation | E, C, then L compute sequentially as transport follows separate routes; the final architecture holds for inspection. |

## 1. Separate saved state from CPU scheduling

The opening introduces C, E, and L as three programs, then shows two distinct
choices: state representation and scheduling. This separates a cooperative
executor's scheduling behavior from the async representation of its tasks.
A conventional thread has a saved execution context and stack. A
stackless async task saves a continuation and the data required to resume. The
choice of scheduler determines when one execution context can displace another.
Concrete systems provide combinations of these properties: Zephyr documents
both cooperative and preemptible threads, while Embassy provides interrupt
executors that can run async tasks at different hardware priorities. See
[Zephyr thread priorities](https://docs.zephyrproject.org/latest/kernel/services/threads/index.html#thread-priorities)
and [Embassy InterruptExecutor 0.10.0](https://docs.rs/embassy-executor/0.10.0/embassy_executor/struct.InterruptExecutor.html).

Chapter 2 switches to compact task tokens to make the memory organization
visible. Watch C's completed work before its context is saved. That work remains
intact while L computes, and C continues from the saved position when selected
again. Both models preserve unfinished computation. The memory difference
concerns how much storage is reserved and what must remain live during
suspension.

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

1. **Run and request.** The explanation identifies C's acquire operation before
   C starts the sensor transfer. A glowing circle follows the device-request
   route.
2. **Wait.** C's card changes to WAITING while its acquire pointer and completed
   work stay in place. The blocking thread retains its call stack; the async
   task retains its continuation state.
3. **Overlap useful work.** L is selected at `encode + buffer`. Its work marks
   advance while its pointer can remain on that operation. The DMA indicator
   moves separately, and C's pointer remains at acquire.
4. **Become ready.** Device completion changes C's label to READY. Its card and
   pointer stay in place. Readiness has changed; CPU ownership has not yet
   changed.
5. **Resume when selected.** CPU selection passes from L to C. C can continue
   its saved operation and advance when that operation completes. The resulting
   state holds so C's pointer and the completed transfer can be compared.

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

In chapter 4, L consumes several already-available packets while C remains
READY. Each packet is small, but the sequence remains one uninterrupted period
of CPU occupation. Watch L's execution marker return to receive and continue
through encoding as the transport circles arrive. C's ready marker remains
still. Only after an operation cannot complete immediately does L become
WAITING and allow another task to execute.

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

The run loops show each role's overall purpose. This numerical trace isolates
one controller job under the assumptions above. The I/O and resource chapters
show separate cases in which waiting must also enter the response argument.

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
it while that poll is still running. L's active operation and growing violet
interval are highlighted while C's program position remains fixed. The
interval passes the 250 µs deadline before C begins. Dispatch takes the stated
5 µs, then C performs the same 80 µs of work used in every case.

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
Its card then changes to PREEMPTED without finishing the highlighted operation.
Inspect the saved pointer and work indicator during the hold. C runs after the
5 µs dispatch interval and completes at 95 µs. The numerical response-time
timeline stops there. L is subsequently selected again and resumes at the same
operation with its completed work preserved.

Using 10 µs as the blocking term assumes that **all applicable lower-priority
blocking is covered by that bound**. Additional locks or masked interrupts
would require evidence and could change it. The later L continuation is a
qualitative demonstration of preservation and resumption. It supplies no second
numerical completion time or bound for L's remaining work; only the first C
job's response is analyzed. Equal 5 µs dispatch costs deliberately isolate the
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

Chapter 8 retains the same three program cards while changing the scheduling
architecture around them. CPU selection identifies one computation at a time;
the programs' purposes and saved positions remain visible.

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

Chapter 10 gives SPI a separate **OWNER: L** label. The resource label, current
CPU owner, and three saved program positions are different facts. Focus first
identifies the dependency, then follows the loop that can resolve it.

1. C executes but cannot obtain the SPI resource already owned by L.
2. C waits at its acquire operation. Medium-priority E computes at
   `update estimate`, while the lower-priority owner L remains at its output
   operation and makes no progress toward releasing the resource.
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

The first half of chapter 11 identifies the wrapped operation before showing
the timer event become ready. Inspect the execution marker: the wrapped
computation is still busy. The event does not become another task that
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

Chapter 12 first makes the distinction concrete with an **illustrative stale
snapshot**: its age is 12 ms, while the allowed age in this example is 5 ms.
Its storage and lifetime can be valid, and a controller can still finish its
calculation on time, while that input violates the stated freshness rule. These
numbers are teaching inputs, not limits prescribed for a particular vehicle.

The example clears before four guarantee layers appear. The table is a set of
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

The last chapter arranges six blocks in two rows. An explanation and focus
highlight identify each source and destination before a glowing circle travels
between them. The short upper routes carry the sensor sample and actuator
command. The estimator's snapshot travels up a separate route. Logging and
monitoring have their own paths so their roles can be followed without crossing
unrelated arrows. Compact task tokens now stand for the programs introduced in
the earlier run-loop panels. Their work advances sequentially—E, then C, then
L—so the functional blocks do not appear to compute in parallel on the one core.

1. **Capture a sample.** The sample pulse enters the control path. Its timestamp
   matters as well as its arrival.
2. **Publish state.** Estimation completes the illustrated work and publishes a
   coherent snapshot. Control needs a specified maximum age and consistency
   contract for the snapshot it consumes.
3. **Compute control.** C accumulates work, then emits a command. The scheduling
   boundary must support the control response bound.
4. **Apply the command.** The command reaches the actuator. End-to-end latency
   includes this path; finishing the control calculation alone is insufficient.
5. **Queue and service background work.** A log pulse enters a bounded queue.
   L performs the illustrated log service later than C's control computation.
   The design needs an explicit overflow and backpressure policy at this
   boundary.
6. **Detect and contain faults.** The monitor has a distinct fault path and
   stated budget. Its ability to act requires the platform and independence
   assumptions represented by that boundary.

The 54-second chapter ends with approximately six seconds of quiet inspection
time. Follow the final sample, snapshot, command, log, and monitor routes and
check the separate timing or resource contract named at each boundary.

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

## Layout with existing Manim-style operations

The scene expresses component relationships through native `VGroup` families,
measured bounds, and relative placement. The Python file still composes ordinary
Noon objects; it does not add a public row, panel, connector, or layout API. The
shared Rust engine owns family bounds, placement, targets, and animation. The
first pass left page anchors and memory handoffs manual; this pass also moves
those relationships into the native layout.

The page starts with a native frame and unpainted header, stage, and caption
regions. These regions and the footer are arranged together; each chapter
places its composition within `self.stage`. The remaining stage height comes
from the frame height, the other regions, and their gaps. Program, memory, and
architecture column widths come from the available stage width and the chosen
number of columns. The scene helpers no longer take positional `x` and `y`
arguments, and chapters no longer repeat independent page coordinates.

The recurring program panels retain named handles to their body, text rows,
execution pointer, work marks, and status slot. Equal panel heights reserve the
tallest measured listing, and the status slot reserves the measured width of
`PREEMPTED`. Each station also has a native content slot below its title; jobs,
queues, and progress tracks use that slot. Those handles give each cue a
semantic target: “the third operation in C” or “the SPI content region.”

| Scene element | Existing operation | What an edit now changes together |
| --- | --- | --- |
| Page regions | `VGroup(...).arrange(DOWN).move_to(self.canvas)` | Header, stage, captions, and footer share the frame's available space. |
| Pseudocode listing | `VGroup(*rows).arrange(DOWN, aligned_edge=LEFT)` | Row spacing and the positions used by the execution pointer. |
| Three program panels | `arrange(RIGHT)` | Panel placement, contents, and the CPU strip positioned below them. |
| Status and CPU labels | `move_to(reserved_slot)` | Replacement text stays in its intended region without moving the panel. |
| Station contents | `next_to(title, DOWN)`, `move_to(content_slot)` | Jobs and device progress use the space below the station heading. |
| Memory handoffs | `arrange`, `copy().match_x(storage_slot)`, `animate.move_to(target)` | Storage bays, CPU regions, and orthogonal turns share native anchors. |
| Execution pointer and strip | `pointer.animate.next_to(row, LEFT)`, `highlight.animate.match_y(row)` | Both destinations use the selected row's measured position. |
| Attention outline | `SurroundingRectangle(target)` | The cue uses the actual text or panel bounds. |
| Deadline trace | `NumberLine.n2p(microseconds)`, `stretch_to_fit_width(..., about_edge=LEFT)` | One numerical mapping places the ticks and cursor and determines each bar's width. |
| DMA progress | `track.point_from_proportion(fraction)` | The progress signal follows the placed track's native path. |
| Architecture and metric panels | `arrange_in_grid` | Rows and columns share deliberate gap settings. |
| Devices, queues, labels, and messages | `next_to`, `move_to`, `match_x`, `match_y` | Related elements are placed from their native anchors. |

For example, `at_line` selects an existing text row and moves both execution
indicators in the same native play call:

```python
item = board[role]
row = item["rows"][index]
await self.play_for(item["pointer"].animate.next_to(row, LEFT, buff=0.18),
                    item["highlight"].animate.match_y(row),
                    *[mark.animate.set_fill(EDGE, opacity=1) for mark in item["marks"]],
                    seconds=seconds)
```

`focus_on` is a small scene helper that creates a native
`SurroundingRectangle`. It controls the tutorial's attention choreography; it
does not calculate a separate layout. The full-width execution strip and the
tighter attention outline remain distinct visual signals.

Chapter 2 now arranges the thread and async columns from their headings,
storage bands, and CPU stations. Equal storage bands align the CPUs while the
schematic stack and state boxes retain their intentionally different heights.
The transfer turns are native copies of the CPU content slot aligned to the
appropriate storage bay. The source expresses a handoff as:

```python
thread_run = thread["cpu"][2]
thread_c_turn = thread_run.copy().match_x(thread["slots"]["C"])
await self.move_job(tc, thread_c_turn, thread["slots"]["C"], seconds=1.0)
```

`move_job` issues ordinary `token.animate.move_to(target)` calls. Moving or
resizing a column before these anchors are created updates both legs of the
handoff without reauthoring stack centers or numeric waypoints. The deadline
bar similarly uses `stretch_to_fit_width(..., about_edge=LEFT)` to preserve its
height and starting edge while revealing the interval; its cursor and work
marks animate concurrently.

The final architecture first arranges six named node families in a two-by-three
grid. Only then does it read the body edges and construct the sample, command,
snapshot, logging, and fault routes. Four routes join facing edges. The logging
elbow uses a deliberate port on C and the midpoint of the space between rows.
Drawing and transport animation reuse the same route points, so the glowing
circle follows the connector that was just introduced. This is explicit
diagram wiring with a reserved corridor, not automatic graph routing.

These layout calls are **one-shot operations**. They measure and place objects
when authored; they do not install persistent constraints. A newly created
`SurroundingRectangle` measures its current target, and a connector's stored
waypoints do not automatically follow nodes moved independently afterward.
This scene therefore lays out components before wiring and creates attention
cues after the relevant placement. The text helper fits measured text within
its stated width by scaling; it does not promise automatic wrapping.

Some numbers remain intentionally: font sizes, panel heights, padding, gaps,
port clearance, marker dimensions, and the trace bar's small positive starting
width. These specify presentation choices and animation geometry. Width
formulas allocate the measured stage or body space among columns and contents.
Microsecond budgets and durations describe the hypothetical execution model or
the lecture's pacing. These have different roles from absolute object
coordinates. Explicit diagram waypoints are derived from already placed node
edges where the topology needs a port or turn. Named handles expose the
relationships that dependent objects use when the scene is authored, reducing
the separate centers and offsets that an edit must keep in sync.

There is a current Noon qualification limit: leaf `align_to` rejects the live
scene context used here. The scene uses the already available
`move_to(..., aligned_edge=...)`, `match_x`, `match_y`, and aligned `next_to`
operations for those placements. This does not establish support for every
Manim API or every live layout combination. The retained-text wrapper needed
one compatibility correction: `Text.move_to` now forwards its alignment and
coordinate-mask arguments unchanged to the existing base placement operation.
The Rust engine and layout calculations are unchanged. There is no Python
frame callback, alternate scene state, or custom layout evaluator.

Reveals also follow the existing native lifecycle. Decorative divider lines
are created from the measured headers after panel arrangement, outside the
layout families, because this revision's leaf `Create` requires a target with
no family parent. The number line is attached for live `n2p` queries, removed,
then revealed with `Create` in the same cue. Neither step adds presentation
time or replaces the native drawing animation. To hide a selection, `hide`
adds `VGroup(*objects)` as one native scene root and applies `FadeOut` to that
family. Scene admission replaces the selected root members, so the fade
removes their scene membership while preserving their layout handles.

## Authoring and verification notes

The scene is authored with ordinary Noon objects and native animations.
Progressive connectors, transport circles, focus highlights, written labels,
and program markers express the lesson's transitions. The shared engine owns
animation and rendering; presentation holds use authored scene time.

### Current layout refactor, second pass

The four focused domain checks passed again after the second layout pass, as
did Python syntax compilation and `git diff --check`. The checks cover response intervals,
the exact-deadline boundary, unchanged execution and dispatch budgets, and the
complete 13-chapter, 522-second cue sheet. Native `NumberLine` geometry is no
longer reimplemented in the domain test.

An AST comparison against the first pass also confirmed unchanged narration
and reading holds. None of the seven component helpers still accepts `x` or
`y`, and no placement call supplies a literal coordinate tuple. Native axis
queries and port/turn calculations remain where the illustration needs
quantitative geometry.

Eight focused text and coordinate-placement tests passed in the first pass for
the unchanged text forwarding correction. That regression first reproduced the
wrapper rejection, then verified that cold and live aligned/masked requests
reach the existing shared-handle operations. These dispatch checks do not
substitute for rendered inspection.

`bash scripts/check.sh fast` was attempted and stopped at its architecture gate
because Cargo is unavailable. Qualified preview setup also failed because the
host has no Docker executable.

A fresh development capture of the current source presented **83 native
1600×900 WebGL2 images**, sampled forward from 0 through 522 seconds. Every
requested time exactly matched its published time. All image hashes and
dimensions were verified, and the final status reports source completion and
an authored duration of exactly 522 seconds. There were no renderer, browser,
or cleanup errors and no blocked HTTP(S) requests.

The authoring team visually inspected **24 selected images spanning all 13
chapters**, including the shared page frame, program board, memory handoffs
and resume, I/O and readiness changes, the three deadline results, saved
preemption progress, runtime boundaries, performance grid, resource and
cancellation diagrams, assurance rows, and final architecture. Settled views
and selected transition interiors were checked; these samples do not establish
continuous-animation smoothness. Full execution also traversed the blank-detail
cancellation caption without a layout or lifecycle error; that specific caption
instant was not separately visually sampled.

The captured scene SHA-256 is
`7968f3242e8270677e66be67a8685c17c094c070c90954e267cce56be18283a7`.
The unchanged WASM and glue come from engine base commit
`b11f450551ef2db4daf99f873b16cab974e4c887`; the regenerated Python worker bundle
includes the text placement forwarding correction. The development build ID is
`5ea3918811f117ee452bf253108233d3f58e2224c23d04ffd92e7fccfa7119b9`.

Capture used the unchanged `SemanticPreviewSession` implementation with its
supported operation deadline set to 120 seconds in the scratch host. Chromium
ran sandboxed under an unprivileged account, with HTTP(S) requests restricted
to loopback and locally served pinned Pyodide. This is trusted-source authored-
time development rendering, not hardened Docker CLI/MCP qualification,
ordinary Run/replay/seek qualification, or a real-time performance benchmark.
The historical movie below remains evidence for its earlier source only.

### Historical revision 3 development export

**The following evidence applies to the pre-refactor revision 3 source and
movie, not to the current layout refactor.** That assembled movie contains
15,660 frames at 30 fps, 1600×900 resolution, a duration of 522 seconds, and
13 chapter markers. Every absolute movie-frame index from 0 through 15,659
occurs exactly once, in order, with the expected presentation timestamp. The
complete movie decoded without errors. The final uninterrupted capture
reached source completion at exactly 522 seconds.

The export's `video_verification.json` records 18,660 actual observations:
15,671 native PNG observations and 2,989 metadata-only forward observations,
including preroll and separate range endpoints. Every requested time matches
its published time. All 10 joins have exact native PNG hash equality; seven
also have direct decoded RGBA comparisons. For the other three, the original
PNG bytes were not retained, so equality is established against the original
recorded native PNG hash. These evidence types remain distinguished in the
verification record.

Runtime interruptions required recovery of completed work. The final movie
uses 7,750 original encoded frames with unchanged AVC payloads, 4,660 frames
encoded from preserved native PNGs, and 3,250 frames from completed fresh
captures. Raw interrupted reports remain unchanged and explicitly interrupted;
recovery does not relabel them as successful complete captures. The evidence
archive retains their identities, ordered-frame checks, and join provenance.

The authoring team individually inspected 37 selected native chapter images,
including the three run-loop overview at 31 seconds and the final architecture
at 520 seconds. Selections use settled captions and complete route labels;
where an earlier overlapping capture was inspected, its PNG hash matches the
image selected for packaging.

The captured scene source SHA-256 is
`ec435450e9c3abef4ae83da4d9fa8245134e3c623099808c957c0f736207e79f`.
Rendering used actual Noon WebGL2 through the unchanged
`SemanticPreviewSession` host, with engine base commit
`b11f450551ef2db4daf99f873b16cab974e4c887` and build ID
`d6cd1c9a0dc2bbc16657db8003ebea4971c5b89ebb122b2089e2579ad69b19ab`.

The historical focused checks for domain arithmetic, the exact-deadline
boundary, the then-authored proportional axis, and the cue sheet were separate
from rendered verification. The current command remains:

```bash
python3 -m unittest web/python/test_async_threads_realtime.py
```

All four focused checks passed for the captured source, as did Python syntax
compilation. The repository validation entry point, `bash scripts/check.sh fast`, was
attempted but stopped at its architecture gate because Cargo was unavailable
locally. Focused domain checks and a development export do not
replace that gate. Hardened Docker preview and ordinary Run/replay/seek
qualification have not been performed.

For the hardened supported preview workflow, follow
[`skills/noon-authoring/references/verification.md`](../../../skills/noon-authoring/references/verification.md)
using a current browser build and the pinned Docker runtime. The following
invocation uses the preserved 522-second cue map and samples near each chapter's
end. It is a preview recipe, not a report that this command has been run:

```bash
NOON_PREVIEW_RUNTIME_CONFIG=/absolute/path/to/runtime.json \
  node tools/noon-mcp/bin/noon-preview.mjs \
  --source /absolute/path/to/async_threads_realtime.py \
  --output /absolute/path/to/review-output \
  --loop-duration 522 --time 31 --time 65 --time 117 --time 157 \
  --time 197 --time 231 --time 275 --time 317 --time 347 \
  --time 393 --time 435 --time 467 --time 521
```

A development capture is a separate category of evidence. Actual Noon canvas
frames captured from the existing `SemanticPreviewSession` host demonstrate
the inspected source, build, times, and backend. A movie encoded from those
frames demonstrates authored-time animation. Neither is automatically a
hardened Docker CLI result, a real-time performance benchmark, or proof of
ordinary Run/replay/seek behavior. Public gallery enrollment also requires the
repository's presentation and playback checks.
