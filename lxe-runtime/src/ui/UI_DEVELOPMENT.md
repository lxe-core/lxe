# LXE UI Development Guide

## Critical: GTK4/GObject Initialization Pattern

### The Problem

In GTK4 with GObject subclassing, `constructed()` runs **BEFORE** `new()` sets custom properties.

```rust
// ⚠️ WRONG PATTERN - NEVER DO THIS
impl ObjectImpl for MyPage {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup_ui();  // ❌ payload_info is EMPTY here!
    }
}

pub fn new(payload_info: Option<PayloadInfo>) -> Self {
    let obj = glib::Object::builder().build();  // triggers constructed()
    *obj.imp().payload_info.borrow_mut() = payload_info;  // Too late!
    obj
}
```

### The Correct Pattern

```rust
// ✅ CORRECT PATTERN - ALWAYS USE THIS
impl ObjectImpl for MyPage {
    fn constructed(&self) {
        self.parent_constructed();
        // NOTE: DO NOT call setup_ui() here!
        // Custom data must be set first in new()
    }
}

pub fn new(payload_info: Option<PayloadInfo>) -> Self {
    let obj = glib::Object::builder().build();
    *obj.imp().payload_info.borrow_mut() = payload_info;
    
    // CRITICAL: setup_ui() AFTER setting data!
    obj.setup_ui();
    
    obj
}
```

## Checklist for New UI Pages

- [ ] `setup_ui()` is called in `new()`, NOT in `constructed()`
- [ ] All custom data is set BEFORE calling `setup_ui()`
- [ ] Add comment in `constructed()` explaining why setup_ui is NOT there
- [ ] Test with a real package to verify metadata displays correctly

## Debug Assertion

All pages should include this assertion at the start of `setup_ui()`:

```rust
fn setup_ui(&self) {
    // Debug assertion: if we have an exe path but no payload, something is wrong
    #[cfg(debug_assertions)]
    {
        let payload = self.imp().payload_info.borrow();
        if payload.is_none() {
            tracing::warn!("setup_ui called with no payload_info - using demo mode");
        }
    }
    // ... rest of setup
}
```

## Why This Happens

The GObject lifecycle in GTK4:
1. `glib::Object::builder().build()` allocates the object
2. `ObjectImpl::constructed()` is called immediately (you can't control this)
3. `build()` returns
4. Your `new()` function continues and sets properties

So if you call `setup_ui()` in step 2, it runs before step 4 sets the data.
