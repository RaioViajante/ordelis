import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { SampleDataProvider } from "./app/SampleData";
import { Shell } from "./app/Shell";
import "./styles/index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <SampleDataProvider>
      <Shell />
    </SampleDataProvider>
  </StrictMode>,
);
