# freyacn-macros

Ergonomic, reactive components for [Freya](https://github.com/marc2332/freya), with a syntax that feels like JSX minus
the angle brackets.

`freyacn-macros` gives you a single `#[component]` attribute that turns a plain Rust function into a fully-fledged UI
component:

- **Props are declared inline** in the attribute, with types, defaults, and required markers — no separate struct to
  write.
- **Every prop is a `Property<T>`** at render time, giving you a uniform API for reading, mutating, and observing
  values.
- **A companion `macro_rules!`** gives you compact call-site syntax:
  `Card!(title = "hi", name = "there")`.
- **Tailwind-flavoured extensions** (`BackgroundExt`, `ForegroundExt`, `SpacingExt`, `SizingExt`, `BorderExt`,
  `EffectsExt`, `CornerRadiusExt`) are implemented on the generated struct, so you can style and compose components with
  the same fluent, utility-class-style API you use on raw Freya elements — `.bg_white()`, `.bg_primary()`,
  `.p_6()`, `.text_pink()`, `.text_sm()`, and so on.
- **Full IDE support for the underlying struct**, so autocompletion on setters and go-to-definition still work if you
  prefer the explicit form.

```rust
use freyacn_macros::*;

#[component(
    title: String,
    name: String = "anonymous".into(),
    required count: i32,
)]
fn Card() {
    rect()
        .child(label(title.get()))
        .child(label(format!("hello, {}", name.get())))
        .child(label(format!("count: {}", count.get())))
}

fn app() -> Element {
    rect()
        .child(Card!(title = "hi", count = 1))
        .child(Card!(title = "bye", name = "ada", count = 2))
}
```

---

## Table of contents

1. [Installation](#installation)
2. [Quick start](#quick-start)
3. [Declaring props](#declaring-props)
4. [Reading props in the body](#reading-props-in-the-body)
5. [Setting props at the call site](#setting-props-at-the-call-site)
6. [The `Property<T>` API](#the-propertyt-api)
7. [Composing components](#composing-components)
8. [Styling components](#styling-components)
9. [Reactivity patterns](#reactivity-patterns)
10. [Reference](#reference)
11. [Design notes](#design-notes)

---

## Installation

```toml
[dependencies]
freyacn-macros = "0.1"
freya = "0.1"
```

The `freyacn-macros` crate re-exports the `#[component]` attribute macro, the
`Property<T>` runtime type, and everything you need from `freya::prelude`.

```rust
use freyacn_macros::*;
```

---

## Quick start

Write a function that returns a `Freya` element, decorate it with
`#[component(...)]`, and list the props you want it to accept:

```rust
use freyacn_macros::*;

#[component(message: String)]
fn Greeting() {
    label(message.get())
}

fn app() -> Element {
    Greeting!(message = "hello, world")
}
```

Three things happened:

1. The macro generated a `GreetingComponent` struct with one field per prop.
2. Each prop is stored as a `Property<String>`, which gives you the uniform read/mutate API described below.
3. A `macro_rules! Greeting` was generated next to the function, so the call site is compact.

If you prefer explicitness, the struct API is available too:

```rust
GreetingComponent::new().message("hello, world")
```

---

## Declaring props

Props are listed inside the `#[component(...)]` attribute, comma-separated. Each entry is one of three forms:

### Optional with default value

```
name: Type = expr
```

The caller may omit the prop. Inside the body, `.get()` returns the fallback if the caller did not set it.

```rust
#[component(
    title: String = "Untitled".into(),
    font_size: f32 = 16.0,
)]
fn Heading() {
    label(title.get()).font_size(font_size.get())
}
```

### Optional with `Type::default()`

```
name: Type
```

Same as above, but the fallback is `<Type as Default>::default()`. Use this when the type has a sensible zero value
(`String`, `bool`, numbers, `Vec`,
`Option`, …).

```rust
#[component(count: i32, subtitle: String)]
fn Badge() {
    // count.get() -> 0 if unset, subtitle.get() -> "" if unset
    label(format!("{} — {}", count.get(), subtitle.get()))
}
```

### Required

```
required name: Type
```

No fallback. If the caller never sets it, the first read panics with a clear message naming the prop:

```
required prop `count` was not set
```

```rust
#[component(required id: u32, label: String)]
fn Item() {
    // id.get() panics if the caller omitted `id`
    // label.get() returns "" if the caller omitted `label`
}
```

### Function parameters become required props

Any parameter on the function itself is also treated as a required prop. This is often the most readable way to declare
required inputs, since the signature documents them and rust-analyzer shows them in autocomplete:

```rust
#[component(count: i32 = 0)]
fn Counter(step: i32) {
    // `step` is required, `count` is optional with default 0
}
```

The two forms are equivalent:

```rust
fn Counter(step: i32) { ... }
// same as
#[component(required step: i32)]
fn Counter() { ... }
```

### Non-`Clone` and event-handler props

Props whose type is not `Clone` — or where you want to route an event through the component — work too. A common case is
a callback:

```rust
use freya::prelude::*;

#[component(
    label: String = "click me".into(),
    on_click: EventHandler<Event<PressEventData>>,
)]
fn Button() {
    let handler = on_click.get();
    rect()
        .on_press(move |e| handler.call(e))
        .child(label.get())
}
```

At the call site, you pass a closure (or any `EventHandler`) directly — no `Arc` wrapping required:

```rust
fn app() -> Element {
    let count = use_signal(|| 0);
    Button!(
        label = "click me",
        on_click = move |_| count.write().add_assign(1),
    )
}
```

`on_click` is declared with a type and no default, which makes it an **optional** prop (`Option<EventHandler<...>>`
under the hood). Inside the body, `on_click.get()` resolves to the caller's handler if one was set, or the type's
`Default` otherwise. If you want to guarantee the caller supplies it, write `required on_click: ...` or move it to the
function signature.

---

## Reading props in the body

Inside the function body, every prop is bound to `&Property<T>`. There is no difference between optional, required, and
defaulted props at this point — they all give you the same methods.

```rust
#[component(
    title: String,
    required id: u32,
    count: i32 = 0,
)]
fn Row() {
    // ---- owned read (requires T: Clone) ----
    let title_owned: String = title.get();

    // ---- zero-copy read ----
    let title_len: usize = title.get_ref().len();

    // ---- closure read, no clone ----
    let upper = title.with(|t| t.to_uppercase());

    // ---- check whether the caller set it ----
    if title.is_set() {
        // caller provided `title`
    }

    // ---- read into an Option ----
    match title.as_option() {
        Some(t) => label(t),
        None => label("(no title)"),
    }

    // ...
}
```

### Choosing between `get`, `get_ref`, and `with`

| Method      | Cost                                        | When to use                                  |
|-------------|---------------------------------------------|----------------------------------------------|
| `get()`     | one clone                                   | you need an owned value                      |
| `get_ref()` | zero-copy, holds a read lock                | you only need to read for a short scope      |
| `with(f)`   | zero-copy, lock released inside the closure | you want to compute something from the value |

`get_ref()` returns a guard that derefs to `&T`. The read lock is held for the guard's lifetime, so keep it short:

```rust
let len = title.get_ref().len();    // guard dropped at `;`
```

`with` is often the cleanest:

```rust
let first_word = title.with( | t| t.split_whitespace().next().map(str::to_owned));
```

### Required props and panics

Reading a required prop that was never set panics. This is the same trade-off
`dioxus` and `rubber_duck` make — the panic happens at render time, not at construction, so a runtime test is the only
way to catch it. If you want compile-time enforcement, keep the required inputs as function parameters and let
rust-analyzer nudge callers.

---

## Setting props at the call site

### The companion macro

Each `#[component]` also generates a `macro_rules!` with the same name as the function. It builds the component by
chaining setters:

```rust
Card!()
Card!(title = "hi")
Card!(title = "hi", name = "there")
Card!(title = "hi", name = "there", count = 5)
```

Order does not matter, and any subset of optional props is valid. The macro expands to a struct expression:

```rust
Card!(title = "hi")
// expands to
CardComponent::new().title("hi")
```

Because `CardComponent` implements the extension traits (see
[Styling components](#styling-components)), you can pass the result directly to
`.child(...)` and chain tailwind-style utilities on it:

```rust
rect().child(Card!(title = "hi"))
rect().child(Card!(title = "hi").bg_white().p_6().text_sm())
```

### The struct API

The generated struct is public and its setters take `impl Into<T>`, so you can skip the macro entirely:

```rust
CardComponent::new()
.title("hi")
.name("there")
.count(5)
```

### Shared state

Setters mutate the underlying `Property` in place via interior mutability, so
`CardComponent` is `Clone` and cloning shares the prop storage:

```rust
let a = CardComponent::new().title("hi");
let b = a.clone();
b.title("changed");
assert_eq!(a.title.get(), "changed");
```

This makes the struct behave like a signal — a `Property<T>` is essentially a typed, scoped `RwLock`.

---

## The `Property<T>` API

Every prop is stored as `Property<T>`, which you receive as `&Property<T>`
inside the render body. The full surface:

### Reading

| Method        | Bound      | Returns                                   |
|---------------|------------|-------------------------------------------|
| `get()`       | `T: Clone` | owned `T`, resolving the fallback         |
| `get_ref()`   | —          | `PropertyRef<'_, T>`, derefs to `&T`      |
| `with(f)`     | —          | whatever `f(&T)` returns                  |
| `as_option()` | `T: Clone` | `Option<T>`, `Some` only if caller set it |
| `is_set()`    | —          | `bool`                                    |
| `is_none()`   | —          | `bool`                                    |

### Writing

| Method          | Effect                                                 |
|-----------------|--------------------------------------------------------|
| `set(v)`        | replace value, mark as set. Takes `&self`.             |
| `clear()`       | reset to fallback (or panic on next read if required)  |
| `into_option()` | consume the `Property`, take the caller's value if any |

### Trait impls

| Trait                            | Notes                                                  |
|----------------------------------|--------------------------------------------------------|
| `Clone`                          | `Arc` bump — no `T: Clone` bound                       |
| `Debug`                          | prints current value and fallback, requires `T: Debug` |
| `PropertyRef: Deref<Target = T>` | enables `&*prop` and method calls                      |
| `PropertyRef: Display`           | forwards to `T: Display`                               |

### Example

```rust
#[component(required id: u32, label: String, count: i32 = 0)]
fn Row() {
    // read
    let l = label.get();
    let n = count.get_ref().to_string();

    // mutate — visible to every clone of `RowComponent`
    count.set(count.get() + 1);

    // reset — falls back to 0
    count.clear();

    // check
    let was_set = label.is_set();

    label(format!("{id} — {l} — {n}"))
}
```

---

## Composing components

Because the companion macro returns a struct that converts into `Element`, composition is straightforward:

```rust
#[component(title: String)]
fn Header() {
    rect().child(label(title.get())).height(Size::px(48.0))
}

#[component(required id: u32, title: String)]
fn Page() {
    rect()
        .child(Header!(title = title.get()))
        .child(Header!(title = "sidebar"))
        .child(main_content(id.get()))
}
```

If you need to pass a prop through without reading it, use `.get()` (owned, requires `Clone`) or restructure to read
once at the top of the body.

### Passing events and closures as props

Handlers are plain props. In Freya, the idiomatic type is
`EventHandler<Event<T>>` — it is `Clone`, cheap to pass around, and accepts ordinary closures at the call site, so you
do not need to reach for `Arc<dyn Fn ...>` yourself.

```rust
use freya::prelude::*;

#[component(
    label: String = "click me".into(),
    on_click: EventHandler<Event<PressEventData>>,
)]
fn Button() {
    let handler = on_click.get();
    rect()
        .on_press(move |e| handler.call(e))
        .child(label.get())
}

fn app() -> Element {
    let count = use_signal(|| 0);
    Button!(
        label = "click me",
        on_click = move |_| count.write().add_assign(1),
    )
}
```

If you do need a plain closure prop, wrap it in `Arc<dyn Fn(...) + Send + Sync>` so it is `Clone`:

```rust
use std::sync::Arc;

#[component(
    label: String,
    on_change: Arc<dyn Fn(String) + Send + Sync>,
)]
fn RawButton() {
    let handler = on_change.get();
    rect().on_press(move |_| handler("pressed".into()))
        .child(label.get())
}
```

---

## Styling components

The generated `Component` struct implements the same tailwind-flavoured extension traits that Freya exposes on built-in
elements. This means a component value returned by `Card!(...)` behaves like any other Freya node: you can chain layout,
paint, and text utilities directly on it, and pass it to `.child(...)` on the parent.

The traits implemented for every generated component struct are:

| Trait             | Representative methods                                                                                   |
|-------------------|----------------------------------------------------------------------------------------------------------|
| `ChildrenExt`     | `.child(...)`, `.children(...)`                                                                          |
| `KeyExt`          | `.key(...)`                                                                                              |
| `BackgroundExt`   | tailwind-style paint: `.bg_white()`, `.bg_primary()`, `.bg_black()`, plus raw `.background(...)`         |
| `ForegroundExt`   | tailwind-style text: `.text_pink()`, `.text_sm()`, `.text_bold()`, `.text_white()`, `.text_primary()`, … |
| `SpacingExt`      | tailwind-style spacing: `.p_6()`, `.p_2()`, `.px_4()`, `.py_3()`, `.m_6()`, `.mt_2()`, …                 |
| `SizingExt`       | tailwind-style sizing: `.w_full()`, `.h_full()`, `.w_auto()`, plus raw `.width(...)`, `.height(...)`     |
| `BorderExt`       | `.border(...)`, per-side borders, plus utility helpers                                                   |
| `EffectsExt`      | `.opacity(...)`, `.blur(...)`, shadows, and other paint effects                                          |
| `CornerRadiusExt` | `.corner_radius(...)`, `.rounded()`, `.rounded_full()`, per-corner variants                              |

The utility methods mirror their Tailwind CSS names as closely as Rust's `snake_case` allows — so `bg-white` becomes
`.bg_white()`, `bg-primary` becomes `.bg_primary()`, `p-6` becomes `.p_6()`, and so on. This keeps the styling
vocabulary identical to the one Freya already uses on raw elements; there is nothing new to memorise.

### Example — a tailwind-flavoured button

```rust
use freya::prelude::*;

#[component(
    label: String = "click me".into(),
    on_click: EventHandler<Event<PressEventData>>,
)]
fn Button() {
    let handler = on_click.get();
    rect()
        .on_press(move |e| handler.call(e))
        .child(label.get())
}

fn app() -> Element {
    rect().child(
        Button!(label = "Save")
            .bg_white()
            .p_6()
            .rounded()
            .text_sm()
            .text_bold()
            .text_pink(),
    )
}
```

Every styling call returns the component by value, so chains stay flat. Under the hood these methods delegate to the
same `BackgroundExt` / `SpacingExt` / `CornerRadiusExt` / `ForegroundExt` machinery Freya uses for `Rect` and friends.

### Key takeaways

- Styling is **outside-in**: props describe *what the component is*, extension traits describe *how it sits in the
  tree*.
- The generated struct is `Clone` and cheap to duplicate, so you can style and re-use the same component value.
- Tailwind-style helpers (`bg_white`, `p_6`, `text_pink`, `rounded`, …) are the recommended styling surface — they read
  the same on a `Button!()` as they do on a `rect()`.

---

## Reactivity patterns

`Property<T>` is not a Freya signal — it is a typed, shared slot used to carry props into the component. Reactive state
lives in your usual
`use_signal` / `use_state` hooks, and `Property<T>` is how you ferry values across component boundaries.

A common pattern is a controlled input:

```rust
#[component(
    value: String,
    on_change: EventHandler<Event<FormEventData>>,
)]
fn TextInput() {
    let on_change = on_change.get();
    rect().child(
        Input::new()
            .value(value.get())
            .on_change(move |e| on_change.call(e)),
    )
}

fn app() -> Element {
    let text = use_signal(String::new);
    TextInput!(
        value = text.read().clone(),
        on_change = move |e| text.set(e.value.clone()),
    )
}
```

If you need mutable state local to the component, use a Freya hook rather than `Property::set` — props are inputs, not
state.

---

## Reference

### Attribute grammar

```
#[component( prop (, prop)* ,? )]

prop := 'required'? name ( ':' Type )? ( '=' expr )?
```

- `name` is an identifier.
- `Type` defaults to the type of the same-named function parameter, if any.
- `expr` is any Rust expression.
- A parameter on the function that is not named in the attribute list is treated as `required name: <param type>`.

### Generated items

For `fn Card(...)`:

| Item                                     | Purpose                                                          |
|------------------------------------------|------------------------------------------------------------------|
| `struct CardComponent`                   | component value; one `Property<T>` field per prop                |
| `impl CardComponent { fn new() }`        | constructor                                                      |
| `impl CardComponent { fn prop(...) }`    | one setter per prop                                              |
| `impl Default for CardComponent`         | delegates to `new()`                                             |
| `impl Component for CardComponent`       | your function body, with prop bindings                           |
| `impl ChildrenExt for CardComponent`     | `.child(...)`, `.children(...)` on the component value           |
| `impl KeyExt for CardComponent`          | `.key(...)`                                                      |
| `impl BackgroundExt for CardComponent`   | `.bg_white()`, `.bg_primary()`, `.background(...)`, …            |
| `impl ForegroundExt for CardComponent`   | `.text_pink()`, `.text_sm()`, `.text_bold()`, `.text_white()`, … |
| `impl SpacingExt for CardComponent`      | `.p_6()`, `.px_4()`, `.py_3()`, `.m_6()`, `.mt_2()`, …           |
| `impl SizingExt for CardComponent`       | `.w_full()`, `.h_full()`, `.width(...)`, `.height(...)`, …       |
| `impl BorderExt for CardComponent`       | `.border(...)`, per-side borders                                 |
| `impl EffectsExt for CardComponent`      | `.opacity(...)`, `.blur(...)`, shadows                           |
| `impl CornerRadiusExt for CardComponent` | `.rounded()`, `.rounded_full()`, `.corner_radius(...)`, …        |
| `macro_rules! Card`                      | companion call-site sugar                                        |

### Crate re-exports

| Item        | From             |
|-------------|------------------|
| `component` | `freyacn_macros` |

---

## Design notes

### Why `Property<T>` instead of `Option<&T>`?

An optional prop would naturally be `Option<&T>`, but reading it ergonomically requires a fallback, and a fallback needs
somewhere to live. `Property<T>`
stores the fallback as a real `T`, so `get_ref()` can hand out a `&T` that outlives any temporary — the same reason
`dioxus::OptionalProp` exists.

### Why a macro for the call site?

Rust has no default or named arguments, so `Card(title = "hi")` is not expressible as an ordinary function. The
companion `macro_rules!` restores the syntax at the cost of IDE inlay hints — rust-analyzer cannot show parameter hints
for arbitrary macro input. If IDE feedback matters more than compactness, use the struct API instead:

```rust
CardComponent::new().title("hi")
```

The struct API gets full autocompletion, hover docs, and go-to-definition — and it exposes the extension-trait methods
just the same.

### Why panic for required props?

Enforcing required props at compile time needs a type-state builder, which means a much larger macro. The runtime panic
is a deliberate trade-off: it costs a render-time crash on a programming error, and it buys a much smaller, more
readable expansion. Required props declared as function parameters get partial compile-time help from rust-analyzer
anyway.

### Why interior mutability?

`Property<T>` is `Arc<RwLock<Option<T>>>`. This makes it `Clone`, `Send +
Sync` when `T` is, and lets setters take `&self` — which in turn lets the generated struct derive `Clone` without any
bounds on `T`. It also matches the way UI frameworks share state across a tree.

### Why implement the Freya extension traits on the generated struct?

Because a component is just a value that eventually lowers to an `Element`, there is no reason to force callers to wrap
or unwrap it before styling. Implementing `ChildrenExt`, `KeyExt`, `BackgroundExt`,
`ForegroundExt`, `SpacingExt`, `SizingExt`, `BorderExt`, `EffectsExt`, and `CornerRadiusExt` on the struct means the
call site reads exactly the same whether you are styling a raw `Rect` or a `Button!()`. It is also what makes
tailwind-style utilities — `bg_white`, `bg_primary`, `p_6`, `text_pink`, `text_sm`, `rounded` — available directly on a
component value, so the styling vocabulary never changes between raw elements and components.

### On `Display` and `Deref` for `PropertyRef`

`PropertyRef` implements `Deref<Target = T>`, so `&*prop` gives `&T` and
`prop.len()` calls through to `T::len`. Pattern matching does not see through `Deref` — use `prop.as_option()` if you
want to match `Some` / `None`.

---

## License

Apache-2.0.