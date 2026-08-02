# nfsm — finite state machines & statecharts

Declarative finite state machines and hierarchical statecharts with guards, transition hooks, and shallow history (~Python `transitions` + `python-statemachine` subset). The graph and transition lookup live in native Rust; guards and callbacks run as Niao callables.

## Import

```niao
import "nfsm"
```

Paths `import "std/nfsm"` and `import "nfsm"` are equivalent. Flat builtins (`nfsm_new`, `nfsm_send`, …) are also available globally after import.

## Quick start

```niao
import "nfsm"

let spec = {
    initial: "idle",
    states: ["idle", "running", "done"],
    final: ["done"],
    triggers: ["start", "finish"],
    transitions: [
        {trigger: "start", source: "idle", dest: "running"},
        {trigger: "finish", source: "running", dest: "done"},
    ],
    hooks: {
        on_enter: {
            running: fn(ctx, e) { print("enter", e.dest) },
        },
    },
}

let m = nfsm.new(spec)
nfsm.send(m, "start")
print(nfsm.state(m))        // "running"
print(nfsm.can(m, "finish")) // true
nfsm.send(m, "finish")
print(nfsm.is_final(m))     // true
nfsm.close(m)
```

## Spec object

| Field | Description |
|-------|-------------|
| `initial` | **Required.** Name of the initial state. |
| `states` | **Required.** Array of state names (strings) or objects `{name, parent?, initial?, final?, history?}`. |
| `triggers` | Trigger/event names. May be omitted if every transition names its `trigger`. |
| `transitions` | Array of transition objects (see below). |
| `final` / `finals` | Names of terminal states. |
| `context` / `model` | Object passed as first argument to guards and hooks (default `{}`). |
| `ignore_invalid` | When `true`, unknown triggers return `nil` instead of an error. |
| `hooks` | Machine-wide callbacks (see Hooks). |

### Transition object

| Field | Aliases | Description |
|-------|---------|-------------|
| `trigger` | `event` | Event name that fires this edge. |
| `source` | `from` | Source state, array of sources, or `"*"` for any. |
| `dest` | `to` | Destination state, `"="` / `"same"`, `"internal"` / `"."`, or `"hist:parent"` for shallow history. |
| `guard` | | Callable `(ctx, event) -> bool`. Must return truthy to allow transition. |
| `unless` | | Callable `(ctx, event) -> bool`. Transition allowed when this returns **false**. |
| `prepare` | | Per-transition prepare hook; return `false` to skip. |
| `on` | | Per-transition action after exits, before enters. |
| `priority` | | Higher wins when multiple edges match (default `0`). |
| `internal` | | `true` for internal transitions (no exit/enter). |

### Hooks object

| Field | Aliases | When invoked |
|-------|---------|--------------|
| `before` | | Before transition; return `false` to cancel. |
| `after` | | After transition completes. |
| `on_prepare` | `prepare` | Machine-level prepare (before per-transition prepare). |
| `on_transition` | `on` | After exits, before enters (machine-level). |
| `on_enter` | `enter` | Map of state name → `fn(ctx, event)`. |
| `on_exit` | `exit` | Map of state name → `fn(ctx, event)`. |

### Event object

Callbacks receive `(context, event)` where `event` is:

| Field | Description |
|-------|-------------|
| `machine` | Handle id. |
| `trigger` / `event` | Trigger name. |
| `source` | Source state name. |
| `dest` | Intended destination name. |
| `transition` | Transition index in the spec. |

## Functions

| Method | Description |
|--------|-------------|
| `nfsm.new(spec)` | Build machine from spec; returns handle (int). |
| `nfsm.close(handle)` | Free handle. Returns `true` if it existed. |
| `nfsm.send(h, trigger)` | Fire trigger; returns `{ok, source, dest, trigger}` or catchable `nfsm_error`. |
| `nfsm.trigger(h, trigger)` | Alias for `send`. |
| `nfsm.state(h)` | Current leaf state name. |
| `nfsm.states(h)` | Active hierarchy leaf→root (array). |
| `nfsm.is_state(h, name)` | `true` if `name` is anywhere in the active hierarchy. |
| `nfsm.is_final(h)` | `true` if current leaf is a final state. |
| `nfsm.can(h, trigger)` | `true` if a transition exists (guards **not** evaluated). |
| `nfsm.triggers(h)` | Triggers with outgoing edges from current state. |
| `nfsm.events(h)` | Alias for `triggers`. |
| `nfsm.reset(h)` | Return to initial configuration. |
| `nfsm.context(h)` | Current context/model object. |
| `nfsm.set_context(h, ctx)` | Replace context; returns `ctx`. |
| `nfsm.history(h)` | Array of `{trigger, source, dest}` records. |
| `nfsm.clear_history(h)` | Clear transition log. |
| `nfsm.count(h)` | Number of completed transitions. |
| `nfsm.ignore_invalid(h, bool)` | Toggle lenient unknown-trigger mode. |
| `nfsm.dot(h)` | Graphviz DOT string (current state bold). |
| `nfsm.info(h)` | `{state, states, triggers, final, count}`. |
| `nfsm.validate(spec)` | Validate spec without creating a machine. |

## Hierarchical states

Composite states use `parent` / `initial` on state objects. Entering a composite automatically enters its `initial` child. Exiting a child updates shallow history for `hist:parent` destinations.

```niao
let spec = {
    initial: "work",
    states: [
        {name: "work", initial: "busy"},
        {name: "busy", parent: "work"},
        "idle",
    ],
    triggers: ["pause", "resume"],
    transitions: [
        {trigger: "pause", source: "busy", dest: "idle"},
        {trigger: "resume", source: "idle", dest: "work"},
    ],
}
let m = nfsm.new(spec)
print(nfsm.state(m))   // "busy"
nfsm.send(m, "pause")
print(nfsm.state(m))   // "idle"
```

## Errors

| Code | Meaning |
|------|---------|
| 3504 | Wrong argument count. |
| 3505 | Invalid transition / guard rejection / spec error (catchable `nfsm_error`). |
| 3506 | Wrong argument type (hard error). |
| 3507 | Invalid or closed handle. |

## Deferred (not in 0.1.0)

- Parallel regions (orthogonal statecharts)
- Deep history
- Queued / async transitions
- Dynamic graph mutation after `new`

## See also

- `nvalid` — schema validation for inputs.
- `nwatch` — reactive polling patterns.
- `nsignal` — OS signal handlers and shutdown guards.
