# Keys: states, gestures and styles

## Multi-state keys

Set a key's type to **Multi-state** to give it 2 to 8 states. Each state has its own label, second line, icon, colour and steps. A press runs the steps of the state that is showing, then the key shows the next state. After the last state it starts again at the first.

The small counter in the corner (for example `2/3`) shows which state is up. The editor preview shows the same state as the deck.

**Follow a variable** makes the key show what is really going on. Pick a variable such as `obs.scene` and give each state the value that means it, for example "Gameplay". When the variable has one of those values, the key shows that state, even when the change came from somewhere else (OBS itself, another key, the HTTP API). When it matches none of them, the key stays where the last press left it.

Values are compared loosely, the same way as "Light up when": `5` matches `5.0`, and text is compared as written.

Multi-state keys and toggles keep their position when Keyside restarts. It is saved in `key-states.json` in the data folder.

## Long press and double tap

Buttons, toggles and multi-state keys can have separate steps for **On long press** and **On double tap**.

- Long press: the steps run once the key has been held for 500 ms (adjustable from 200 ms to 5 s), while the finger is still down. A shorter tap runs the normal steps when the key is let go.
- Double tap: two taps within 300 ms (adjustable from 100 ms to 1 s) run the double tap steps, right on the second tap. A single tap then runs the normal steps once the window has passed, so only keys with double tap steps get this short wait.
- A long press or a double tap doesn't move a toggle or a multi-state key on.

Keys that need to act the moment they go down can't use either: **Hold to confirm**, **Repeat while held**, and steps that act while held, such as "Talk while held" or "Unmute while held". The editor turns these options off for each other and says why. Steps that act while held do work as the long press or double tap itself.

This works the same on phones and on Stream Decks plugged into the PC. Keys pressed through the HTTP API always run their normal steps.

## Key styles

A style is a named rule: **when** a variable matches, keys using the style change their colour, light up, icon, badge or label. Open **Key styles** from the editor menu to create, change and delete styles, and pick styles for a key under **Styles** in its settings. A key can use up to 8 styles, and a layout can have up to 64.

Styles apply in the order the key lists them, before the key's own "Light up when" rule, so a key can still override a shared style. Deleting a style also takes it off every key that used it.

Styles belong to the whole layout file. Share codes carry pages and keys, not styles, so imported keys come without them.
