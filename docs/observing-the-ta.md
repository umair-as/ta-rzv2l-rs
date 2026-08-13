# Observing the TA at runtime

How to watch the signer work on the board, and — just as important — what each tool cannot
see. The recurring trap is drowning in output: a single `sign` produces tens of thousands of
trace lines, almost all of it plumbing. The skill is extracting the few hundred bytes of
signal, not reading the log.

## Three vantage points

A TA runs in the secure world, whose *execution* a normal-world tracer cannot follow. (That is
a tracing limit, not a memory-isolation guarantee — on this board, which runs without secure
boot, normal-world root can still read secure DRAM directly, a separate matter covered in
[`security-model.md`](security-model.md).) So no single tool shows everything; each sits at a
different boundary.

| Tool | What it sees | What it needs |
|---|---|---|
| Linux **ftrace** | the normal-world side: the OP-TEE driver, the SMC world-switch, and every RPC to `tee-supplicant` | nothing — available on the board now |
| OP-TEE **secure console** | the TA's own `trace_println!` / OP-TEE `IMSG`/`DMSG` lines | console access and a permissive TA log level |
| OP-TEE **ftrace** (`CFG_FTRACE_SUPPORT`) | a function-graph *inside* the TA's own execution | OP-TEE core and the TA rebuilt with `-pg` instrumentation |

Linux ftrace never crosses into the TA. When the driver makes the SMC into the secure world,
ftrace sees the call go in and the return come out; the execution in between is a black box to
the *tracer*. That black box is the limit of what a normal-world tracer can follow — seeing the
gap is part of the lesson. It is not a claim that the secure world's memory is unreachable: on
this board it is reachable (see above). Tracing visibility and memory isolation are different
properties, and only the first one holds here.

## Tracing the REE side without drowning in it

Set the tracer to follow only the OP-TEE driver, run one command, then **stop tracing before
reading anything**:

```sh
cd /sys/kernel/tracing
echo function_graph > current_tracer
echo 'optee_*' > set_graph_function      # the kernel expands this to every optee_ symbol
echo 1 > tracing_on
#   (second shell)  signer-client sign <64-hex digest>
echo 0 > tracing_on
```

Now the buffer holds ~58,000 lines. **Do not `cat` it.** `function_graph` descends into every
child a function calls — schedulers, spinlocks, memory — so the vast majority is normal-world
plumbing filling the waits. The signal is a handful of function names. Extract it by counting,
not reading:

```sh
# 1. Call volume by function — the shape of the interaction in ~15 lines.
grep -oE 'optee_[a-z_]+' trace | sort | uniq -c | sort -rn | head

# 2. The application-level operations, in order, with line numbers.
grep -nE 'optee_(open_session|invoke_func|close_session)' trace

# 3. The one number that matters most: round-trips to tee-supplicant.
grep -c optee_supp_thrd_req trace

# 4. RPC accounting — how those round-trips split across phases. Use the line
#    numbers from step 2 as the range boundaries.
sed -n '<openLine>,<nextLine>p' trace | grep -c optee_supp_thrd_req
```

(`rg` works identically on a dev host; the board ships `grep`, so the recipe uses that.)

Step 4 is where understanding comes from: `optee_supp_thrd_req` is one request from OP-TEE to
the normal world — load a TA file, read a storage object. Counting it *between* the app-level
operations shows exactly where the cost of a TEE call actually lands.

## A worked capture

One `pubkey` run and one `sign` run, captured together. From the board, the whole interaction
in four numbers:

```text
optee_open_session          2      # two client runs
optee_invoke_func           3      # pubkey (1) + sign (fetches pubkey, then signs = 2)
optee_close_session         2
optee_smc_do_call_with_arg  12     # logical calls into the secure world
optee_smccc_smc            109     # actual world-switches — ~9 per logical call
optee_supp_thrd_req        116     # round-trips to tee-supplicant
```

The 116 supplicant round-trips, split by phase (step 4 above):

| Phase | Supplicant RPCs | What it is |
|---|---:|---|
| first `open_session` → first `invoke` | **66** | **TA loading** — reading the `.ta` from `/lib/optee_armtz/` in chunks |
| second `open_session` → its `invoke` | **0** | TA already resident — no reload |
| `invoke` → `invoke` (GetPubkey) | **25** | secure-storage read |
| `invoke` → `close` (Sign) | **25** | secure-storage read |

## What the numbers mean

Three findings fall straight out of that table, and each is a design property made visible:

- **TA loading is the expensive part, and it is paid once.** The first session spent 66
  round-trips having the supplicant read the TA binary. The second session spent **zero** —
  because the signer is `SINGLE_INSTANCE`, so once resident OP-TEE reuses it. The flag set in
  the TA's build config is doing exactly what it promises, measurably.

- **Each storage-touching command costs ~25 round-trips**, because secure storage here is
  REE-FS: the encrypted key blob is a file on the Linux filesystem, so reading it means OP-TEE
  asking the supplicant for the object and its hash-tree nodes. This is the number that would
  change shape if the board ever moved key storage to RPMB.

- **The TA re-reads the key on every command.** GetPubkey and Sign each cost ~25 RPCs because
  both call the same load-from-storage path rather than caching the key in session context. For
  an infrequently-called signer that is a reasonable trade — always fresh from storage, nothing
  lingering — but the trace makes the cost explicit rather than hidden.

## What you cannot see, and the honest gap

- The ~28,000 trace lines between the first `open_session` and the next event are **not** TEE
  operations. They are normal-world code running under the SMC/RPC waits. The signal in that
  span is the 66 `optee_supp_thrd_req` markers; everything else is plumbing.
- You never see inside the TA — no key generation, no signing, no key material. That is correct
  and expected: the secure world is opaque to a normal-world tracer.
- ftrace uses a fixed-size ring buffer, so a long capture overwrites its own start. In this run
  the very first session's `open_session` had already scrolled off the top. Capture the smallest
  window you can — start tracing immediately before the command, stop immediately after.

## Going further

- `echo funcgraph-proc funcgraph-abstime > trace_options` before the run tags each line with a
  pid and an absolute timestamp, so the 66-RPC load can be *timed* against the 25-RPC storage
  read — where the milliseconds actually go.
- To watch the supplicant's file I/O directly — the `.ta` open, the `/var/lib/tee/` reads —
  `strace -f -p "$(pidof tee-supplicant)"` is clearer than ftrace for that one question.
- The only way to trace *inside* the TA is OP-TEE's own ftrace (`CFG_FTRACE_SUPPORT`, TA built
  with `-pg`), which dumps a function-graph of the TA's execution via the supplicant. It needs
  an OP-TEE and TA rebuild, so it pairs naturally with other firmware-side work. This REE-side
  capture is the right thing to compare it against when that lands.
