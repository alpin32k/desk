import { App } from "./app";
import "./styles.css";

const root = document.querySelector<HTMLElement>("#app")!;
new App(root).start().catch((err) => {
  root.textContent = `desk: ${err}`;
});
