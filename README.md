# `freyacn-macros`

Procedural macros for the [`freyacn`][freyacn] crate. They turn plain functions into styled, prop‑driven components
for [Freya][freya], with Tailwind‑inspired styling built in and complete generated documentation.

The crate exposes three attributes:

* **`#[component]`** — turns a function into a component struct plus a PascalCase constructor function and a companion
  macro.
* **`#[extensions(...)]`** — opts the generated struct into one or more of freyacn's built‑in extension bundles
  (`children`, `key`, `style`, `event_handlers`).
* **`#[struct_fields(...)]`** — adds internal storage fields to the generated struct, without exposing them as props.
  Use this to store data your own custom trait impls need.

All three work together; `#[component]` is the recommended entry point because it knows how to initialise every field.

---

## Installation

```toml
[dependencies]
freyacn = "0.1"
freya = "0.4"
```

```rust
use freyacn::{StyleExt, component, extensions, struct_fields};
use freya::prelude::*;
```

---

## `#[component]`

Every component is written as a normal Rust function. The macro inspects the function's signature and the attribute
argument to decide each prop's kind.

### Three kinds of props

| Where you write it                               | Kind          | Field type       | Initial value       | Body sees                   | Setter accepts              |
|--------------------------------------------------|---------------|------------------|---------------------|-----------------------------|-----------------------------|
| `fn Card(name: String)`                          | **required**  | `Option<String>` | `None`              | `&String` (panics if unset) | `impl Into<String>`         |
| `#[component(name: String)]`                     | **optional**  | `Option<String>` | `None`              | `&Option<String>`           | `impl Into<Option<String>>` |
| `#[component(name: String = "ada".to_string())]` | **defaulted** | `String`         | `"ada".to_string()` | `&String`                   | `impl Into<String>`         |

You **never** write `Option<T>` yourself for an optional prop — declaring
`name: String` in the attribute is enough; the macro wraps it in `Option`.

A name appearing both as a function parameter and in the attribute is a compile error.

### Example

```rust
use freyacn::{component, StyleExt};
use freya::prelude::*;

// Required — from the function signature.
#[component]
fn Greeting(name: String) {
    // `name: &String`, panics at render if never set.
    label().text(format!("Hello, {name}!")).into()
}

// Optional — attribute entry, no default.
#[component(subtitle: String)]
fn Heading(subtitle: String) {
    // `subtitle: &Option<String>`
    match subtitle {
        Some(s) => label().text(s.as_str()).into(),
        None => rect().into(),
    }
}

// Defaulted — attribute entry with `= expr`.
#[component(size: f32 = 16.0, weight: String = "normal".to_string())]
fn Text(size: f32, weight: String) {
    label().text(format!("{}pt {}", size, weight)).into()
}
```

### Construction

Four equivalent ways to build a component:

```rust
let a = Greeting().name("Ada");               // fn constructor
let b = GreetingComponent::new().name("Ada"); // struct ctor
let c = Greeting!(name = "Ada");              // macro
let d = GreetingComponent {                   // struct literal
name: Some("Ada".to_string()),
};
```

### Blanket `From`/`Into` for optional props

Optional setters take `impl Into<Option<T>>`. Because std provides:

```rust
impl<T> From<T> for Option<T> { /* Some */ }
impl<T> From<T> for T { /* identity */ }
```

all of these compile:

```rust
h.subtitle("hi");                       // &str → Some("hi".into())
h.subtitle(String::from("hi"));         // String → Some
h.subtitle(Some("hi".to_string()));     // Option<String>
h.subtitle(None);                       // Option<String>
```

For required and defaulted props the setter takes `impl Into<T>`, so passing
`None` is a type error — which is exactly the point:

```rust
g.name("Ada");                          // OK
// g.name(None);                        // ← does not compile
// g.name(Some("Ada".to_string()));     // ← does not compile
```

---

## `#[struct_fields(...)]`

Adds internal storage fields to the generated struct. These fields are **not props**:

* No setter is generated.
* They are documented under a separate "Struct fields" section in the generated docs.
* They're bound as locals in the render body, exactly like props.

The point of `#[struct_fields(...)]` is to give your own custom trait impls somewhere to store data on the component —
without inventing a wrapper, a `RefCell<HashMap>`, or a proc‑macro registry.

### Grammar

Same as `#[component(...)]`:

```text
field := ident ":" type [ "=" expr ]
list  := field { "," field } [ "," ]
```

* Without `= expr` → field type becomes `Option<T>`, initialised to `None`.
* With `= expr`     → field type stays `T`, initialised with `expr`.

### Example

```rust
use freyacn::{component, extensions, struct_fields, StyleExt};
use freya::prelude::*;

#[component(subtitle: String, badge: String = "new".to_string())]
#[struct_fields(
    tooltip: MyTooltip,             // Option<MyTooltip> = None
    hover_count: usize = 0,         // usize = 0
)]
#[extensions(children, key, style)]
fn Card(name: String) {
    // name:         &String              (required — fn param)
    // subtitle:     &Option<String>      (optional — attribute)
    // badge:        &String              (defaulted — attribute)
    // tooltip:      &Option<MyTooltip>   (struct field)
    // hover_count:  &usize               (struct field)
    // children:     &Vec<Element>        (extension)
    // key:          &DiffKey             (extension)
    // style:        &Style               (extension)

    let tip_text = tooltip.as_ref().map(|t| t.text.as_str()).unwrap_or("");

    rect()
        .background(style.background)
        .padding(style.padding)
        .child(label().text(format!("{name} — {badge} (hovered {hover_count}×)")))
        .child(label().text(tip_text))
        .children(children.clone())
        .into()
}
```

The user's own trait impl reads and writes the field directly, in plain Rust:

```rust
#[derive(Default)]
pub struct MyTooltip {
    pub text: String,
    pub visible: bool,
}

pub trait TooltipExt {
    fn set_tooltip(&mut self, text: impl Into<String>);
}

impl TooltipExt for CardComponent {
    fn set_tooltip(&mut self, text: impl Into<String>) {
        let tip = self.tooltip.get_or_insert_with(MyTooltip::default);
        tip.text = text.into();
    }
}
```

Call site:

```rust
let mut card = Card()
.name("Account")
.subtitle("Personal info")   // auto-wrapped in Some
.badge("verified")
.bg_white()
.p_6();

card.set_tooltip("Click to edit");
card.hover_count = 7;            // struct fields are public
```

### Why not just use a prop?

You could declare `tooltip: MyTooltip = MyTooltip::default()` inside `#[component(...)]`. The differences:

|                  | Prop                | Struct field            |
|------------------|---------------------|-------------------------|
| Setter generated | Yes                 | No                      |
| Documented as    | Public API          | Internal detail         |
| Declared with    | `#[component(...)]` | `#[struct_fields(...)]` |

The two attributes make the *intent* explicit. A reader of `#[component(name: String)]` knows `name` is part of the
component's public API. A reader of `#[struct_fields(tooltip: MyTooltip)]` knows it's implementation detail managed by
an extension impl.

---

## `#[extensions(...)]`

Opt the generated struct into one or more of freyacn's built‑in extension bundles.

| Name             | Trait                    | Field(s) added                                           | Trait method         |
|------------------|--------------------------|----------------------------------------------------------|----------------------|
| `children`       | `freyacn::ChildrenExt`   | `children: Vec<Element>`                                 | `get_children`       |
| `key`            | `freyacn::KeyExt`        | `key: DiffKey`                                           | `write_key`          |
| `style`          | `freyacn::StyleExt`      | `style: Style`                                           | `get_style`          |
| `event_handlers` | `freyacn::EventHandlers` | `event_handlers: FxHashMap<EventName, EventHandlerType>` | `get_event_handlers` |

Extension fields are appended **after** all props and struct fields. **Any** prop or struct field that shares a name
with an extension field is a compile error.

### Example

```rust
use freyacn::{component, extensions, StyleExt};
use freya::prelude::*;

#[component(title: String, badge: String = "new".to_string())]
#[extensions(children, key, style)]
fn Card(name: String) {
    // name:     &String              (required — fn param)
    // title:    &Option<String>      (optional — attribute)
    // badge:    &String              (defaulted — attribute)
    // children: &Vec<Element>        (extension)
    // key:      &DiffKey             (extension)
    // style:    &Style               (extension)
    rect()
        .background(style.background)
        .padding(style.padding)
        .child(label().text(format!("{name} — {badge}")))
        .children(children.clone())
        .into()
}
```

Every helper the `StyleExt` trait provides becomes available on the generated struct:

```rust
let card = Card()
.name("Profile")
.title("Personal info")          // auto-wrapped in Some
.badge("updated")
.bg_card()
.text_card_foreground()
.p_6()
.gap_4()
.corner_radius(12.0)
.shadow_md()
.class("font-semibold");         // class-string API
```

---

## Forwarding user attributes

Every attribute you place on the function other than `#[doc]`, `#[extensions(...)]`, and `#[struct_fields(...)]` is
forwarded **verbatim** to the generated struct. This means you decide what derives and marker attributes the struct
carries — the macro never bakes any in.

```rust
#[component(label: String)]
#[struct_fields(cache: Vec<String> = Vec::new())]
#[derive(Clone, Debug, PartialEq, Hash)]
#[cfg(feature = "experimental")]
#[my_custom_attribute(arg = 42)]
fn Widget(label: String) { /* … */ }
```

The generated struct carries the same `#[derive(...)]`, `#[cfg(...)]`, and custom attribute, in the same order they
appeared above the function. Doc comments on the function become the struct's docs; a generated summary (listing props,
struct fields, and extensions) is appended after them.

Three attributes are **not** forwarded, because they are handled by the macro itself:

* `#[doc]` — re‑emitted first, so it isn't duplicated.
* `#[extensions(...)]` — consumed and turned into fields and trait impls.
* `#[struct_fields(...)]` — consumed and turned into storage fields.

If you want `Clone` or `PartialEq` on the struct, add `#[derive(Clone, PartialEq)]` yourself. The macro won't guess, and
downstream extensions that need those bounds will produce a normal "trait bound not satisfied" error pointing at the
generated impl — which is where the user can fix it by adding the derive.

---

## Attribute ordering

`#[component]` consumes `#[extensions(...)]` and `#[struct_fields(...)]`, so both must appear **below** `#[component]`:

```rust
#[component(name: String)]        // ✓ correct
#[struct_fields(tooltip: MyTooltip)]
#[extensions(children, style)]
fn Card(name: String) { /* … */ }
```

If you place them above, a stub attribute fires with a friendly error explaining the correct order:

```rust
#[struct_fields(tooltip: MyTooltip)]   // ✗ wrong
#[component(name: String)]
fn Card(name: String) { /* … */ }
```

```
error: `#[struct_fields(...)]` must be placed **below** `#[component(...)]`:

       #[component(name: String)]
       #[struct_fields(tooltip: MyTooltip)]
       fn Card(name: String) { … }

       Attributes are processed top-to-bottom; `#[component]` consumes
       `#[struct_fields]` and turns it into one or more struct fields. If
       you place `#[struct_fields]` first, the compiler reaches it before
       `#[component]` has had a chance to run.
```

---

## Generated code

For:

```rust
#[component(subtitle: String)]
#[struct_fields(tooltip: MyTooltip, hover_count: usize = 0)]
#[extensions(children, key, style)]
#[derive(Clone, Debug, PartialEq)]
fn Card(name: String) { /* … */ }
```

the macro emits:

```rust
// User doc comments, then the generated summary, then user attributes.
#[derive(Clone, Debug, PartialEq)]          // ← forwarded from the user
pub struct CardComponent {
    // ── Attribute props ──
    pub subtitle: Option<String>,           // optional

    // ── Struct fields ──
    pub tooltip: Option<MyTooltip>,         // optional
    pub hover_count: usize,                 // with default

    // ── Required prop (fn param) ──
    pub name: Option<String>,               // required

    // ── Extension fields ──
    pub children: Vec<Element>,
    pub key: DiffKey,
    pub style: Style,
}

impl CardComponent {
    pub fn new() -> Self { /* … */ }

    // One setter per prop — no setters for struct fields or extension fields.
    pub fn name(self, value: impl Into<String>) -> Self { /* … */ }
    pub fn subtitle(self, value: impl Into<Option<String>>) -> Self { /* … */ }
}

impl Default for CardComponent { /* delegates to new() */ }

impl freyacn::ChildrenExt for CardComponent { /* … */ }
impl freyacn::KeyExt for CardComponent { /* … */ }
impl freyacn::StyleExt for CardComponent { /* … */ }

impl freyacn::Component for CardComponent {
    fn render(&self) -> impl freyacn::IntoElement {
        let children: &Vec<Element> = &self.children;
        let key: &DiffKey = &self.key;
        let style: &Style = &self.style;

        let subtitle: &Option<String> = &self.subtitle;
        let tooltip: &Option<MyTooltip> = &self.tooltip;
        let hover_count: &usize = &self.hover_count;

        let name: &String = self.name.as_ref()
            .expect("required prop `name` was not set before render");

        /* original function body */
    }
}

pub fn Card() -> CardComponent { CardComponent::new() }

#[macro_export]
macro_rules! Card {
    ($($key:ident = $val:expr),* $(,)?) => {
        CardComponent::new()$(.$key($val))*
    };
}
```

Every item carries generated documentation describing the props, struct fields, and extensions applied — so
`cargo doc` produces a complete reference for free.

---

## Compile errors

The macro emits pointed errors for common mistakes.

### Name in both fn param and attribute prop

```rust
#[component(name: String = "ada".to_string())]
fn Card(name: String) { /* … */ }
```

```
error: `name` is declared more than once (a function parameter and an attribute
       prop (`#[component(...)]`)). Each field name may be used by exactly one
       declaration.
```

### Missing type in `#[component(...)]` or `#[struct_fields(...)]`

```rust
#[component(name)]
fn Card() { /* … */ }
```

```
error: missing type — every field must declare its type, e.g. `name: String`.
       (Required props are declared via the function signature instead.)
```

### `required` keyword

```rust
#[component(required name: String)]
fn Card(name: String) { /* … */ }
```

```
error: `required` is not a keyword. Function parameters are the only way to
       declare required props: `fn Card(name: String)`. Attribute props are
       optional (or defaulted with `= expr`).
```

### Collision with an extension field

```rust
#[component(children: Vec<Element>)]
#[extensions(children)]
fn Broken() { /* … */ }
```

```
error: `children` collides with a field contributed by `#[extensions(...)]`.
       Extension fields are reserved by freyacn and cannot be shadowed by an
       attribute prop (`#[component(...)]`).
```

```rust
#[struct_fields(children: Vec<Element>)]
#[extensions(children)]
fn Broken() { /* … */ }
```

```
error: `children` collides with a field contributed by `#[extensions(...)]`.
       Extension fields are reserved by freyacn and cannot be shadowed by a
       struct field (`#[struct_fields(...)]`).
```

### Same name declared twice

```rust
#[component(tooltip: String)]
#[struct_fields(tooltip: MyTooltip)]
fn Broken() { /* … */ }
```

```
error: `tooltip` is declared more than once (an attribute prop
       (`#[component(...)]`) and a struct field (`#[struct_fields(...)]`)).
       Each field name may be used by exactly one declaration.
```

---

## Standalone `#[extensions(...)]`

For hand‑written structs that are not generated by `#[component]`:

```rust
use freyacn::extensions;

#[extensions(children, key)]
#[derive(Clone, PartialEq, Default)]
pub struct PlainBox;
```

The macro appends the requested fields and their trait impls. The struct must derive or implement `Default` so the added
fields can be initialised.

---

## Full example

```rust
use freyacn::{component, extensions, struct_fields, StyleExt};
use freya::prelude::*;

/// A labelled card with optional subtitle, children, styling and a custom
/// tooltip that the user's own trait impl manages.
#[component(subtitle: String, badge: String = "new".to_string())]
#[struct_fields(tooltip: MyTooltip, hover_count: usize = 0)]
#[derive(Clone, PartialEq)]
#[extensions(children, key, style)]
fn Card(name: String) {
    let tip_text = tooltip.as_ref().map(|t| t.text.as_str()).unwrap_or("");

    rect()
        .background(style.background)
        .corner_radius(style.corner_radius)
        .padding(style.padding)
        .child(
            rect()
                .child(label().text(name.as_str()))
                .child(match subtitle {
                    Some(s) => label().text(s.as_str()).into(),
                    None => rect().into(),
                })
                .child(label().text(tip_text)),
        )
        .children(children.clone())
        .into()
}

#[derive(Default)]
pub struct MyTooltip {
    pub text: String,
    pub visible: bool,
}

pub trait TooltipExt {
    fn set_tooltip(&mut self, text: impl Into<String>);
}

impl TooltipExt for CardComponent {
    fn set_tooltip(&mut self, text: impl Into<String>) {
        let tip = self.tooltip.get_or_insert_with(MyTooltip::default);
        tip.text = text.into();
    }
}

fn app() -> IntoElement {
    let mut card = Card()
        .name("Account")
        .subtitle("Personal information")   // auto-wrapped in Some
        .badge("verified")
        .bg_white()
        .p_6()
        .gap_4()
        .corner_radius(12.0)
        .shadow_md()
        .class("border-2 border-slate-200")
        .child(label().text("Settings go here"));

    card.set_tooltip("Click to edit");
    card.hover_count = 3;

    card.into()
}
```

---

## Design notes

**Why does each source decide the prop kind?**
Keeping required props in the signature means the compiler enforces them — you can't forget to declare them, and the
render body can rely on `&T` without unwrapping. Optional props live in the attribute because their type (`Option<T>`)
is a wrapping decision the macro can make for you. Defaulted props live in the attribute because the default expression
*is* an attribute‑only concept.

**Why `impl Into<Option<T>>` for optional setters?**
Because std already gives us `From<T> for Option<T>` and `From<T> for T`, so the same setter accepts a bare `T`
(auto‑wrapped), an `Option<T>` (identity), and `None` — all with a single signature and no runtime overhead.

**Why is `#[struct_fields(...)]` separate from `#[component(...)]`?**
Because the two declare different things. `#[component(...)]` declares the component's **public API** — each entry
becomes a chainable setter, is documented as a prop, and is part of the surface other code uses. `#[struct_fields(...)]`
declares **internal storage** — no setter, documented separately, used only by the component's own trait impls. Merging
them would force users to pick a documentation section that misrepresents one of the two.

**Why a compile error instead of a silent override for name collisions?**
Silent override hides a real bug: the user has declared the same thing twice. Refusing to compile is the loudest,
earliest signal.

**Why are extension fields appended after props and struct fields?**
Props and struct fields are what the user wrote; extensions are opt‑in machinery. Putting user‑written declarations
first keeps the common case at the top of the struct.

**Why generate a free function *and* a macro with the same name?**
They live in different Rust namespaces (value vs macro), so there's no conflict. The function is the zero‑argument fast
path; the macro adds named‑argument construction. Both are documented.

**Why forward user attributes instead of baking in derives?**
Because the right set of derives depends on how the component is used: a component used as a React‑style keyed child
needs `PartialEq`; one stored in a collection needs `Clone`; one inspected during debugging needs `Debug`. Baking any of
them in forces users to accept them and hides the cost. Forwarding keeps the macro agnostic — the user writes what they
want and sees the exact set of traits the struct implements in its source. The same mechanism transparently supports
`#[cfg(...)]`, custom attribute macros, and marker attributes without the macro having to know about them.

**Why is there a stub `#[struct_fields]` proc‑macro?**
Because attribute macros are processed top‑to‑bottom. If `#[struct_fields(...)]` is placed above `#[component(...)]`,
the compiler reaches `struct_fields` first — and without a stub, it errors with "cannot find attribute". The stub exists
only to produce a friendly message. When the ordering is correct, `#[component]` consumes the attribute before the
compiler ever sees the stub, so the stub never fires.

---

## License

Apache‑2.0.

[freyacn]: https://crates.io/crates/freyacn
[freya]: https://crates.io/crates/freya