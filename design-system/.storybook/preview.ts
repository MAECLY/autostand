import "../tokens/tokens.css";
import type { Preview } from "@storybook/react";

const preview: Preview = {
  parameters: {
    controls: { matchers: { color: /(background|color)$/i, date: /Date$/i } },
    backgrounds: {
      values: [
        { name: "light", value: "var(--bg-base)" },
        { name: "dark", value: "var(--color-slate-950)" },
      ],
    },
    layout: "padded",
  },
};

export default preview;