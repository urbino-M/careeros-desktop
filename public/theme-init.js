// Apply the saved palette before rendering, without requiring inline-script CSP.
try {
  document.documentElement.dataset.theme = localStorage.getItem("careeros-theme") === "light" ? "light" : "dark";
} catch {
  document.documentElement.dataset.theme = "dark";
}
