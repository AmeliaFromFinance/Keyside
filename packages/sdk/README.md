# @phonedeck/sdk

Write [PhoneDeck](https://github.com/OWNER/phonedeck) plugins in TypeScript.

```sh
npm create phonedeck-plugin my-plugin
```

```ts
import { definePlugin } from "@phonedeck/sdk";

export default definePlugin({
  actions: {
    hello: {
      press({ key }) {
        key.set({ badge: "HI" });
      },
    },
  },
});
```

Test without PhoneDeck:

```ts
import { createTestHost } from "@phonedeck/sdk/testing";
```

Full guide: [docs/plugin-guide.md](https://github.com/OWNER/phonedeck/blob/main/docs/plugin-guide.md)

MIT licensed.
