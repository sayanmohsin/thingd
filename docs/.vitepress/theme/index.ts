import DefaultTheme from "vitepress/theme";
import { h } from "vue";
import ParticleBackground from "./components/ParticleBackground.vue";
import "./custom.css";

export default {
  extends: DefaultTheme,
  Layout() {
    return h(DefaultTheme.Layout, null, {
      "layout-top": () => h(ParticleBackground),
    });
  },
};
