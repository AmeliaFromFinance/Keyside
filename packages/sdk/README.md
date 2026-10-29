# @keyside/sdk

Write [Keyside](https://github.com/AmeliaFromFinance/keyside) plugins in TypeScript.

```sh
npm create keyside-plugin my-plugin
```

```ts
import { definePlugin } from "@keyside/sdk";

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

Test without Keyside:

```ts
import { createTestHost } from "@keyside/sdk/testing";
```

Full guide: [docs/plugin-guide.md](https://github.com/AmeliaFromFinance/keyside/blob/main/docs/plugin-guide.md)

MIT licensed.
