---
name: leptos-component
description: Write Leptos components and islands with correct prop declarations, signal types, view! macro usage, and reactivity primitives (RwSignal/Memo/Effect). Auto-triggers when adding or editing any #[component] or #[island] function, declaring props, or using signals/effects in Leptos 0.8+ under islands mode.
---

# Leptos Component & Reactivity Primitives

> **Islands rule (see [[leptos-islands]]):** under islands mode any function whose body contains `on:click`, `on:input`, `on:change`, `on:submit`, `RwSignal::new`, `signal(...)`, `Memo::new`, `Effect::new`, or a `ServerAction` MUST be `#[island]`, not `#[component]`. `#[component]` is server-only with zero client reactivity — handlers never attach, signals never update, effects never run.
>
> The reactivity primitives in this skill (`RwSignal`, `Memo`, `Effect`, signal splitting) work inside an island. Interactive examples below use `#[island]`. Pure static prop/composition examples (`UserCard`) stay `#[component]`.

## When to Use

Invoke before:
- Writing or editing any `#[component]` or `#[island]` function.
- Declaring props (required, optional, defaulted, children).
- Choosing between `RwSignal`, split signals, `Memo`, or `Effect`.
- Encountering hydration mismatches suspected to originate in component markup.
- Deciding whether a function with reactive primitives belongs in a `#[component]` or `#[island]` — under islands mode the answer is `#[island]` (see [[leptos-islands]]).

## Core Patterns

### Pattern 1 — Component Declaration and Props

`#[component]` marks every hydrating component. Props are declared as function parameters. Required props have no default. Optional props use `Option<T>` or `#[prop(optional)]`.

```rust
use leptos::prelude::*;

#[component]
pub fn UserCard(
    /// Required: user's display name
    name: String,
    /// Optional with default: show avatar?
    #[prop(default = true)]
    show_avatar: bool,
    /// Optional: additional CSS classes
    #[prop(optional)]
    class: Option<String>,
    /// Children slot
    children: Children,
) -> impl IntoView {
    view! {
        <div class=format!("card {}", class.unwrap_or_default())>
            {show_avatar.then(|| view! { <img src=format!("/avatars/{name}.jpg") alt=name.clone() /> })}
            <span>{name}</span>
            {children()}
        </div>
    }
}
```

Anti-pattern — `String` prop with no clone when used multiple times in view:
```rust
// BAD: name is moved into first use; second use is a compile error
view! {
    <h1>{name}</h1>       // name moved here
    <p>{name}</p>         // ← use of moved value
}
// FIX: clone before first use or use a signal / store
```

### Pattern 2 — Combined vs Split Signals

`RwSignal::new(v)` is the default (combined read+write, cloneable). Use split signals (`signal(v)`) only at API boundaries where you want to expose only read or only write access.

`Counter` is `#[island]` — it owns reactive state and an `on:click` handler. The `count_badge` helper is a **plain function** (not `#[island]`, not `#[component]`) that takes the parent's `ReadSignal` and composes into the island's view tree. This is the correct decomposition for an interactive subtree that depends on parent-island signals (see [[leptos-islands]] "Interactive Helper Functions" and Anti-pattern 7).

```rust
#[island]
pub fn Counter(initial: i32) -> impl IntoView {
    // Combined signal for the parent (read + write).
    let count = RwSignal::new(initial);
    // Expose a read-only view to a child helper at the API boundary.
    let read_only = Signal::derive(move || count.get());

    view! {
        <button on:click=move |_| count.update(|n| *n += 1)>
            "Count: " {count}
        </button>
        {count_badge(read_only)}
    }
}

// Plain function. NOT #[island] (would fragment ownership), NOT #[component] (would
// freeze the signal value at SSR). Composes into the calling island's tree.
fn count_badge(count: Signal<i32>) -> impl IntoView {
    view! { <span class="badge">{count}</span> }
}
```

Anti-pattern — `RwSignal<T>` for derived state:
```rust
// BAD: derived value stored in an RwSignal — caller can mutate it,
//      and it goes stale when source changes
let doubled = RwSignal::new(count.get() * 2);  // ← not reactive!

// CORRECT: Memo caches and recomputes automatically
let doubled = Memo::new(move |_| count.get() * 2);
```

### Pattern 3 — Memo for Derived State

`Memo::new(|_| expr)` memoizes a derived value. It re-runs only when tracked dependencies change. Never use `RwSignal<T>` for computed values.

`filtered_list` here is a **plain function**, not `#[component]` and not `#[island]`. It is called from inside a parent island as `{filtered_list(items, filter)}` and composes into that island's view tree. The reactive signal reads work because the surrounding island is hydrated; the helper itself is purely a code-organization boundary (see [[leptos-islands]] "Interactive Helper Functions").

If the parent caller is a `#[component]` (server-only) the signal reads do not update client-side — but the bug is in the caller, not in this helper. Promote the caller to `#[island]`; do not promote this helper to `#[island]` (that would fragment the ownership tree — see [[leptos-islands]] Anti-pattern 7).

```rust
pub fn filtered_list(items: ReadSignal<Vec<String>>, filter: ReadSignal<String>) -> impl IntoView {
    // Recomputes only when items or filter changes
    let filtered = Memo::new(move |_| {
        let f = filter.get();
        items.get().into_iter().filter(|s| s.contains(&f)).collect::<Vec<_>>()
    });

    view! {
        <ul>
            <For each=move || filtered.get() key=|s| s.clone() let:item>
                <li>{item}</li>
            </For>
        </ul>
    }
}
```

Anti-pattern — recomputing inside `view!` on every render:
```rust
// BAD: filter logic runs on every reactive update of anything in the component
view! {
    <ul>
        {move || items.get().into_iter().filter(|s| s.contains(&filter.get())).map(|i| view!{<li>{i}</li>}).collect_view()}
    </ul>
}
// CORRECT: extract to Memo — runs only when items or filter changes
```

### Pattern 4 — Effect for Browser-Only Side Effects

`Effect::new(|_| ...)` runs client-side only after hydration. Use it as the correct escape hatch for browser-specific code (DOM manipulation, localStorage, analytics). **Effects only run inside hydrated islands.** Placing `Effect::new` in a `#[component]` is silent dead code — the effect never executes.

```rust
#[island]
pub fn ScrollTracker() -> impl IntoView {
    let position = RwSignal::new(0_f64);

    // Runs only in the browser, after hydration
    Effect::new(move |_| {
        let cb = Closure::wrap(Box::new(move || {
            let y = window().scroll_y().unwrap_or(0.0);
            position.set(y);
        }) as Box<dyn Fn()>);
        window().add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref()).ok();
        cb.forget();
    });

    view! { <span>"Scroll: " {position} "px"</span> }
}
```

Anti-pattern — `cfg!(target_arch = "wasm32")` inside `view!`:
```rust
// BAD: cfg!() is evaluated at compile time but the view! branches still exist in
//      both builds — hydration sees different DOM trees → mismatch panic
view! {
    <div>
        {if cfg!(target_arch = "wasm32") {   // ← FORBIDDEN inside view!
            view! { <span>"browser"</span> }.into_any()
        } else {
            view! { <span>"server"</span> }.into_any()
        }}
    </div>
}
// CORRECT: use Effect::new for browser-only code; use SSR-streamed signals for content
```

### Pattern 5 — Valid HTML in view!

The browser auto-corrects invalid HTML during parsing. Leptos' SSR emits the structure you wrote. When they differ, hydration fails. Always write structurally valid HTML.

```rust
// CORRECT: table with required tbody
view! {
    <table>
        <thead>
            <tr><th>"Name"</th><th>"Score"</th></tr>
        </thead>
        <tbody>
            <For each=rows key=|r| r.id let:row>
                <tr><td>{row.name}</td><td>{row.score}</td></tr>
            </For>
        </tbody>
    </table>
}
```

Anti-pattern — `<table>` without `<tbody>`:
```rust
// BAD: browser inserts <tbody> automatically; SSR output has none → hydration mismatch
view! {
    <table>
        <tr><td>"data"</td></tr>   // ← no <tbody> wrapper
    </table>
}
```

## Anti-Patterns to Block

- `#[component]` containing `on:click` / `on:input` / `on:change` / `on:submit` / `RwSignal::new` / `Memo::new` / `Effect::new` / `ServerAction::<…>::new()` → must be `#[island]` (see [[leptos-islands]] Anti-pattern 3). Under islands mode handlers do not attach to `#[component]` markup; the UI ships dead.
- A trigger button in one `#[island]` opening a modal/sheet rendered from a sibling `#[component]` → trigger + content + form + buttons must live in the same island subtree (see [[leptos-islands]] "Modals, Sheets, Dialogs").
- `cfg!(target_arch = "wasm32")` inside `view!` → move browser-only logic to `Effect::new` or use compile-time `#[cfg]` at the module level.
- `RwSignal<T>` for a value derived from another signal → use `Memo::new(|_| ...)`.
- `<table>` without `<tbody>` in `view!` → always include `<tbody>`; browser inserts it, SSR doesn't.
- Block element (`<div>`, `<p>`, `<ul>`) inside `<p>` → invalid HTML, causes hydration mismatch.
- `name` (or any `String` prop) used multiple times in `view!` without `.clone()` → clone before first move into the closure.
- `Effect::new` reading a `Resource` → read Resources inside `<Suspense>` only.
- Forgetting `move` on closures in `view!` → compile error; always `move |_| ...`.

## Verification Hooks

```bash
# Detect interactive markup inside #[component] (must be #[island] under islands mode)
for f in $(grep -rln '#\[component\]' src/); do
  if grep -qE 'on:(click|input|change|submit)|RwSignal::new|Memo::new|Effect::new|ServerAction::<' "$f"; then
    echo "  ↑ interactive code inside #[component] — promote to #[island]: $f"
  fi
done

# Detect cfg!(target_arch) inside view! macros
grep -rn 'cfg!(target_arch' src/

# Detect <table> without <tbody>
grep -rn '<table' src/ | grep -v '<tbody'

# Detect RwSignal used to store results of a computation (heuristic)
grep -rn 'RwSignal::new(.*\.get()' src/

# Detect Effect reading a Resource (pattern: resource.get() inside Effect)
grep -rn -A20 'Effect::new' src/ | grep '\.get()'

# Run clippy on WASM target
cargo clippy --no-default-features --features=hydrate --target wasm32-unknown-unknown -- -D warnings
```

## References

- https://book.leptos.dev/03_components.html
- https://book.leptos.dev/05_forms.html
- https://book.leptos.dev/reactivity/ (§4 — signals, memos, effects)
- https://docs.rs/leptos/latest/leptos/attr.component.html
- https://book.leptos.dev/14_async/04_suspense.html (§14.4 hydration bugs)
