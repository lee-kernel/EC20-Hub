import { createApp } from "vue";
import App from "./App.vue";
import TrayPopup from "./components/TrayPopup.vue";
import "./style.css";

const params = new URLSearchParams(window.location.search);
const rootComponent = params.get("view") === "tray" ? TrayPopup : App;

createApp(rootComponent).mount("#app");
