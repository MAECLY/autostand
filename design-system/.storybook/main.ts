import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  stories: ["../**/*.stories.tsx"],
  addons: ["@storybook/addon-essentials"],
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  viteFinal: async (config) => {
    const tailwindcss = (await import("@tailwindcss/vite")).default;
    config.plugins = [...(config.plugins || []), tailwindcss()];
    return config;
  },
};

export default config;