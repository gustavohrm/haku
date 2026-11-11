---
status: APPROVED
last_updated: 2026-11-11
scope: How Haku's interface is laid out and how its flows behave, for every popover, panel, menu and internal page.
---

# Interface guidelines

Haku is a browser for getting work done, so its interface should be calm and fast: it shows what is needed for the task at hand, and gets out of the way once the task is done. These rules say how. [Code guidelines § Styling](code.md#styling) owns which classes and surfaces to draw with; this document owns what goes on them and how they behave. The [Brand](brand.md) owns colour and type.

Approved by the maintainer on 2026-11-11. Code that predates it is legacy until updated.

## Surfaces

- **Flat.** A menu, popover, panel or toast is opaque, on the `chrome-overlay` surface, with a thin border and no shadow. Without a blur, a page showing through competes with the text ([Chrome layering § Flat surfaces over the page](../specs/chrome-layering.md#flat-surfaces-over-the-page)). A translucent surface is an exception, and says why where it is drawn.
- **One surface per task.** A popover does not open a second popover for one of its fields; a menu anchored to the field is fine, since it closes as soon as a choice is made.

## Popovers and panels

- **No heading that repeats what opened it.** A popover opened from the star is about the bookmark; a heading saying "Bookmark" spends a line on nothing. A heading stays when the popover's subject is not evident from its anchor, and then it names that subject, as a site menu names the site.
- **Fields name themselves.** Where an icon or the value already says what a field is, it has no visible label; it keeps an accessible name. A field whose value could be mistaken for another's keeps a label.
- **One primary action.** At most one filled button, on the right, which is what Enter does. Other actions are quiet: ghost or text buttons, on the left when they undo or leave, such as **Remove**.
- **Changes apply as they are made** where they can be undone by changing them back, so closing is never a decision. **Done** only closes.
- **Esc and a click away close.** A popover editing something that exists keeps what was changed. A popover creating something discards it: keeping it would create a half-typed bookmark or engine. A popover that asks a question needing an answer does not close this way.

## Lists

- **Each item reads as one line**: its name first, then what distinguishes it, muted. A value that is set rarely shows as text, not as an always-open field.
- **Actions appear on the row they act on**, on hover and focus, and the row's menu offers them too. The row stays quiet the rest of the time. A row in a passing state keeps the actions that state is about in view, such as a running download's **Pause** and **Cancel**: they are why the row is being looked at.
- **Edit in place.** Editing a row opens it where it is, as a small form; saving or Esc closes it back into a row. A list does not keep a second copy of a row in an editor elsewhere.
- **Adding starts from a button.** **Add …** opens the same in-place form at the end of the list. A form for adding is not on screen until it is asked for.
- **Hidden items fold away** under one line that says how many there are, rather than mixing with what is in use.
- **Order is changed by dragging** a row's handle, and from the keyboard with the arrow keys on that handle. Arrow buttons on every row are noise.

## Forms

- **Ask for the least.** When what was typed shows which kind of value it is, Haku decides and shows its guess as a small, changeable tag, instead of asking first.
- **Validate when saving**, and say what is wrong under the field it concerns, in the field's own words. Never drop an input silently.
- **Confirm in place.** A destructive step that needs confirming asks in the form or row it started from, saying exactly what will go, such as how many visits. A dialog is for a question that cannot wait for the row, such as one from a page.

## Menus

- A menu ends with the way out to where the thing is managed, such as **Manage bookmarks**, after a separator.
- A submenu's first item is level with the item that opened it, and its edge meets its parent's ([Context menus § The menu component](../specs/context-menus.md#the-menu-component)).

## Wording

- Labels are short and say what happens: **Remove**, not **Remove this bookmark from your bookmarks**. A description says what is not obvious, such as what is sent and to whom.
- Name things the same everywhere: an action called **Remove** in one place is not **Delete** in another for the same effect.
