// The pages work as plain forms; this adds what GitHub adds on top: menus that close,
// Write/Preview tabs, checkboxes you can tick, labels applied from a menu, cards dragged between
// the board's columns, live refresh when a file changes on disk, and a few keyboard shortcuts.
(() => {
  "use strict";

  const csrf = document.querySelector('meta[name="csrf-token"]')?.content ?? "";
  let leaving = false;
  // A card is being dragged or moved: a reload now would drop it.
  let holding = false;

  const post = (url, data) =>
    fetch(url, {
      method: "POST",
      headers: { "X-CSRF-Token": csrf, Accept: "application/json" },
      body: new URLSearchParams(data),
      credentials: "same-origin",
    });

  const openMenus = () => [...document.querySelectorAll("details.js-menu[open]")];

  // Banner at the bottom of the page

  const banner = document.querySelector(".live-banner");
  const showBanner = (text) => {
    if (!banner) return;
    banner.querySelector(".js-banner-text").textContent = text;
    banner.hidden = false;
  };
  banner?.querySelector(".js-reload")?.addEventListener("click", () => {
    leaving = true;
    location.reload();
  });
  banner?.querySelector(".js-banner-close")?.addEventListener("click", () => {
    banner.hidden = true;
  });

  // Unsaved text

  const typedText = () =>
    [...document.querySelectorAll("textarea, input[type=text]")].some(
      (field) =>
        !field.matches(".js-search-input, .js-label-filter") &&
        !field.closest("[hidden]") &&
        field.value !== field.defaultValue,
    );
  const busy = () =>
    typedText() ||
    openMenus().length > 0 ||
    document.querySelector(".js-edit-form:not([hidden]), .js-title-form:not([hidden])") !== null;

  window.addEventListener("beforeunload", (event) => {
    if (!leaving && typedText()) event.preventDefault();
  });

  document.addEventListener("submit", (event) => {
    const form = event.target;
    if (form.dataset.confirm && !window.confirm(form.dataset.confirm)) {
      event.preventDefault();
      return;
    }
    leaving = true;
  });

  // Menus close on a click outside, on Escape, and when another one opens

  document.addEventListener("click", (event) => {
    for (const menu of openMenus()) if (!menu.contains(event.target)) menu.open = false;
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape") return;
    for (const menu of openMenus()) {
      menu.open = false;
      menu.querySelector("summary")?.focus();
    }
  });
  document.addEventListener(
    "toggle",
    (event) => {
      const menu = event.target;
      if (!menu.matches?.("details.js-menu") || !menu.open) return;
      for (const other of openMenus()) if (other !== menu && !other.contains(menu)) other.open = false;
    },
    true,
  );

  // Write / Preview

  document.addEventListener("click", async (event) => {
    const tab = event.target.closest(".js-editor-tab");
    if (!tab) return;
    const editor = tab.closest(".js-editor");
    const input = editor.querySelector(".js-editor-input");
    const preview = editor.querySelector(".js-editor-preview");
    const previewing = tab.dataset.tab === "preview";
    for (const other of editor.querySelectorAll(".js-editor-tab")) {
      other.classList.toggle("selected", other === tab);
      other.setAttribute("aria-selected", String(other === tab));
    }
    input.hidden = previewing;
    preview.hidden = !previewing;
    if (!previewing) {
      input.focus();
      return;
    }
    if (!input.value.trim()) {
      preview.innerHTML = '<p class="muted">Nothing to preview</p>';
      return;
    }
    preview.innerHTML = '<p class="muted">Loading preview…</p>';
    try {
      const response = await post(input.dataset.previewUrl, { text: input.value });
      if (!response.ok) throw new Error(String(response.status));
      // The server renders Markdown with raw HTML escaped, the same way it renders the page.
      preview.innerHTML = (await response.json()).html;
    } catch {
      preview.textContent = "The preview could not be loaded. Reload the page and try again.";
    }
  });

  // Ctrl/Cmd + Enter submits, with the form's primary button

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" || !(event.metaKey || event.ctrlKey)) return;
    const form = event.target.closest?.("form");
    if (!form || !event.target.matches("textarea, input[type=text]")) return;
    event.preventDefault();
    const primary = form.querySelector('button.btn-primary[type="submit"]');
    if (primary && !primary.disabled) form.requestSubmit(primary);
  });

  // Ticking a checkbox writes that line of the file

  const setVersion = (version) => {
    document.body.dataset.version = version;
    for (const input of document.querySelectorAll('input[name="version"]')) input.value = version;
  };

  document.addEventListener("change", async (event) => {
    const box = event.target;
    if (!box.matches("input.task-list-item-checkbox[data-line]")) return;
    const container = box.closest("[data-tasks-url]");
    box.disabled = true;
    try {
      const response = await post(container.dataset.tasksUrl, {
        part: box.dataset.part,
        line: box.dataset.line,
        checked: box.checked ? "1" : "0",
        version: document.body.dataset.version,
      });
      const data = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(data.error || "The checkbox could not be saved.");
      setVersion(data.version);
      if (box.dataset.part === "description") {
        for (const summary of document.querySelectorAll(".js-task-summary")) {
          summary.textContent = `${data.done} of ${data.total} tasks`;
        }
      }
    } catch (error) {
      box.checked = !box.checked;
      showBanner(error.message);
    } finally {
      box.disabled = false;
    }
  });

  // Editing the description, a comment, or the title

  const resetEditor = (form) => {
    form.reset();
    for (const tab of form.querySelectorAll('.js-editor-tab[data-tab="write"]')) tab.click();
  };

  document.addEventListener("click", (event) => {
    const opener = event.target.closest(".js-edit-open");
    if (opener) {
      const form = document.getElementById(opener.dataset.target);
      form.closest(".timeline-comment").querySelector(".js-comment-body").hidden = true;
      form.hidden = false;
      opener.closest("details").open = false;
      const input = form.querySelector("textarea");
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
      return;
    }
    const cancel = event.target.closest(".js-edit-cancel");
    if (cancel) {
      const form = cancel.closest("form");
      resetEditor(form);
      form.hidden = true;
      form.closest(".timeline-comment").querySelector(".js-comment-body").hidden = false;
    }
  });

  const titleShow = document.querySelector(".js-title-show");
  const titleForm = document.querySelector(".js-title-form");
  document.querySelector(".js-title-edit")?.addEventListener("click", () => {
    titleShow.hidden = true;
    titleForm.hidden = false;
    const input = titleForm.querySelector('input[name="title"]');
    input.focus();
    input.select();
  });
  document.querySelector(".js-title-cancel")?.addEventListener("click", () => {
    titleForm.reset();
    titleForm.hidden = true;
    titleShow.hidden = false;
  });

  // The comment form: "Close with comment" once there is text, no empty comments

  for (const form of document.querySelectorAll(".js-comment-form")) {
    const input = form.querySelector("textarea");
    const closeButton = form.querySelector(".js-close-button");
    const commentButton = form.querySelector(".js-comment-button");
    const update = () => {
      const hasText = input.value.trim() !== "";
      if (closeButton) {
        const label = closeButton.querySelector(".js-close-label");
        label.textContent = hasText ? closeButton.dataset.textLabel : closeButton.dataset.emptyLabel;
      }
      if (commentButton) commentButton.disabled = !hasText;
    };
    input.addEventListener("input", update);
    update();
  }

  // Label picker: filter, create, and on the issue page apply when the menu closes

  const pillStyle = (dot) => {
    const colour = getComputedStyle(dot).backgroundColor;
    const [red, green, blue] = (colour.match(/\d+/g) ?? ["237", "237", "237"]).map(Number);
    const light = (red * 0.2126 + green * 0.7152 + blue * 0.0722) / 255 > 0.453;
    return `background-color:${colour};color:${light ? "#1f2328" : "#ffffff"}`;
  };

  for (const picker of document.querySelectorAll(".js-label-picker")) {
    const filter = picker.querySelector(".js-label-filter");
    const list = picker.querySelector(".js-label-list");
    const createButton = picker.querySelector(".js-label-create");
    const createName = picker.querySelector(".js-label-create-name");
    const empty = picker.querySelector(".js-label-empty");
    const options = () => [...list.querySelectorAll(".label-option")];
    const chosen = () =>
      options()
        .filter((option) => option.querySelector("input").checked)
        .map((option) => option.querySelector("input").value);
    let initial = chosen().join("\n");

    const showChosen = () => {
      const target = picker.parentElement.querySelector(".js-chosen-labels");
      if (!target) return;
      target.replaceChildren();
      const picked = options().filter((option) => option.querySelector("input").checked);
      if (!picked.length) {
        const none = document.createElement("span");
        none.className = "muted";
        none.textContent = "None yet";
        target.append(none);
      }
      for (const option of picked) {
        const pill = document.createElement("span");
        pill.className = "IssueLabel";
        pill.setAttribute("style", pillStyle(option.querySelector(".color-dot")));
        pill.textContent = option.querySelector("input").value;
        target.append(pill, " ");
      }
    };

    const applyFilter = () => {
      const text = filter.value.trim().toLowerCase();
      let exact = false;
      for (const option of options()) {
        option.hidden = text !== "" && !option.dataset.name.includes(text);
        exact ||= option.dataset.name === text;
      }
      createButton.hidden = text === "" || exact;
      createName.textContent = filter.value.trim();
      if (empty) empty.hidden = text !== "" || options().length > 0;
    };

    const create = () => {
      const name = filter.value.trim().replace(/\s+/g, " ");
      if (!name) return;
      const existing = options().find((option) => option.dataset.name === name.toLowerCase());
      if (existing) {
        existing.querySelector("input").checked = true;
      } else {
        const option = document.createElement("label");
        option.className = "menu-item label-option";
        option.dataset.name = name.toLowerCase();
        const box = document.createElement("input");
        box.type = "checkbox";
        box.name = "labels";
        box.value = name;
        box.checked = true;
        if (picker.dataset.form) box.setAttribute("form", picker.dataset.form);
        const check = list.querySelector(".menu-check")?.cloneNode(true) ?? document.createElement("span");
        check.className = "menu-check";
        const dot = document.createElement("span");
        dot.className = "color-dot";
        dot.setAttribute("style", "background-color:#ededed");
        const text = document.createElement("span");
        text.className = "label-option-name";
        text.textContent = name;
        option.append(box, check, dot, text);
        list.insertBefore(option, createButton);
      }
      filter.value = "";
      applyFilter();
      showChosen();
      filter.focus();
    };

    filter.addEventListener("input", applyFilter);
    filter.addEventListener("keydown", (event) => {
      if (event.key !== "Enter") return;
      event.preventDefault(); // Enter in the filter never submits the form
      const text = filter.value.trim().toLowerCase();
      if (!text) return;
      const visible = options().filter((option) => !option.hidden);
      const exact = visible.find((option) => option.dataset.name === text);
      if (exact || (visible.length === 1 && createButton.hidden)) {
        const box = (exact ?? visible[0]).querySelector("input");
        box.checked = !box.checked;
        filter.value = "";
        applyFilter();
        showChosen();
      } else {
        create();
      }
    });
    createButton.addEventListener("click", create);
    list.addEventListener("change", showChosen);

    picker.addEventListener("toggle", () => {
      if (picker.open) {
        initial = chosen().join("\n");
        filter.value = "";
        applyFilter();
        filter.focus();
        return;
      }
      if (picker.classList.contains("js-auto-submit") && chosen().join("\n") !== initial) {
        const form = document.getElementById(picker.dataset.form);
        leaving = true;
        form.requestSubmit();
      }
    });
  }

  // Copy a path or a timestamp

  document.addEventListener("click", async (event) => {
    const button = event.target.closest(".js-copy");
    if (!button) return;
    try {
      await navigator.clipboard.writeText(button.dataset.copy);
      button.classList.add("copied");
      setTimeout(() => button.classList.remove("copied"), 1500);
    } catch {
      window.prompt("Copy:", button.dataset.copy);
    }
    button.closest("details.js-menu")?.removeAttribute("open");
  });

  // Keyboard shortcuts: / for search, c for a new issue

  document.addEventListener("keydown", (event) => {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.target.closest?.("input, textarea, select, [contenteditable]")) return;
    if (event.key === "/") {
      const search = document.querySelector(".js-search-input");
      if (!search) return;
      event.preventDefault();
      search.focus();
      search.select();
    } else if (event.key === "c") {
      const link = document.querySelector(".js-new-issue");
      if (!link) return;
      event.preventDefault();
      location.href = link.href;
    }
  });

  // The board: drag a card to another column, and the issue file gets that column's label

  const board = document.querySelector(".js-board");
  if (board) {
    let dragged = null;
    const zones = () => [...board.querySelectorAll(".js-drop")];
    const icons = document.querySelector(".js-state-icons")?.content;

    const recount = () => {
      const totals = {};
      for (const zone of zones()) {
        const column = zone.closest(".board-column");
        const total = zone.children.length + Number(column.dataset.hidden || 0);
        totals[zone.dataset.column] = total;
        column.querySelector(".js-column-count").textContent = total;
      }
      const show = (selector, value) => {
        const target = document.querySelector(selector);
        if (target) target.textContent = value;
      };
      show(".js-count-waiting", totals.backlog + totals.review + totals.blocked);
      show(".js-count-backlog", totals.backlog);
      show(".js-count-review", totals.review);
      show(".js-count-blocked", totals.blocked);
      show(".js-count-in-progress", totals["in-progress"]);
    };

    const settle = (card, column, version) => {
      card.dataset.version = version;
      const input = card.querySelector('input[name="version"]');
      if (input) input.value = version;
      for (const button of card.querySelectorAll(".js-move")) button.hidden = button.value === column;
      card.querySelector(".js-inferred")?.remove();
      const open = column !== "done";
      const icon = icons?.querySelector(`[data-open="${open}"]`);
      const state = card.querySelector(".js-state");
      if (icon && state) state.innerHTML = icon.innerHTML;
    };

    const clearTargets = () => {
      for (const zone of board.querySelectorAll(".drop-target")) zone.classList.remove("drop-target");
    };

    board.addEventListener("dragstart", (event) => {
      const card = event.target.closest?.(".js-card");
      if (!card) return;
      dragged = card;
      holding = true;
      card.classList.add("dragging");
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", card.id);
    });
    board.addEventListener("dragend", () => {
      dragged?.classList.remove("dragging");
      if (dragged && !dragged.classList.contains("moving")) holding = false;
      dragged = null;
      clearTargets();
    });
    board.addEventListener("dragover", (event) => {
      const zone = event.target.closest?.(".board-column")?.querySelector(".js-drop");
      if (!dragged || !zone) return;
      event.preventDefault();
      event.dataTransfer.dropEffect = "move";
      if (!zone.classList.contains("drop-target")) {
        clearTargets();
        zone.classList.add("drop-target");
      }
    });
    board.addEventListener("drop", async (event) => {
      const zone = event.target.closest?.(".board-column")?.querySelector(".js-drop");
      const card = dragged;
      if (!card || !zone) return;
      event.preventDefault();
      clearTargets();
      const from = card.parentElement;
      if (zone === from) return;
      const next = card.nextElementSibling;
      zone.prepend(card);
      card.classList.add("moving");
      recount();
      try {
        const response = await post(card.dataset.statusUrl, {
          status: zone.dataset.column,
          version: card.dataset.version,
        });
        const data = await response.json().catch(() => ({}));
        if (!response.ok) throw new Error(data.error || "The card could not be moved.");
        settle(card, data.column, data.version);
        // The board's own move is not a change to reload for.
        const versionUrl = document.body.dataset.versionUrl;
        const fresh = await fetch(versionUrl, { headers: { Accept: "application/json" }, cache: "no-store" });
        if (fresh.ok) document.body.dataset.version = (await fresh.json()).version;
      } catch (error) {
        from.insertBefore(card, next);
        recount();
        showBanner(error.message);
      } finally {
        card.classList.remove("moving");
        holding = false;
      }
    });
  }

  // Live refresh: poll a cheap version endpoint and reload when a file changed on disk

  const versionUrl = document.body.dataset.versionUrl;
  const seconds = Number(document.body.dataset.refreshSeconds || 0);
  if (versionUrl && seconds > 0) {
    let checking = false;
    setInterval(async () => {
      if (document.hidden || checking || leaving || holding) return;
      checking = true;
      try {
        const response = await fetch(versionUrl, { headers: { Accept: "application/json" }, cache: "no-store" });
        if (!response.ok) return;
        const { version } = await response.json();
        if (version === document.body.dataset.version) return;
        if (version === null) {
          showBanner("This issue's file is no longer on disk.");
        } else if (busy()) {
          showBanner("This changed on disk. Reload to see the latest version; copy any text you typed first.");
        } else {
          leaving = true;
          location.reload();
        }
      } catch {
        // The viewer may be restarting; the next check will try again.
      } finally {
        checking = false;
      }
    }, seconds * 1000);
  }
})();
