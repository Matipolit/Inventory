window.addEventListener("load", () => {
  // Wire up each dialog with its own open and close buttons
  document.querySelectorAll("dialog").forEach((dialog) => {
    const id = dialog.id; // e.g. "dialog-5"
    const showButton = document.getElementById(`button-${id}`);
    const closeButton = dialog.querySelector("button.btn-danger");

    if (showButton) {
      showButton.addEventListener("click", () => dialog.showModal());
    }
    if (closeButton) {
      closeButton.addEventListener("click", () => dialog.close());
    }
  });
});
