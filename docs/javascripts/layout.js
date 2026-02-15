(function () {
  const COLLAPSE_CLASS = "kagan-sidebar-collapsed";
  let scheduled = false;

  function getActiveTopLevelItem(primaryNav) {
    return primaryNav.querySelector(":scope > .md-nav__list > .md-nav__item--active");
  }

  function shouldCollapsePrimarySidebar() {
    const primarySidebar = document.querySelector('.md-sidebar--primary[data-md-type="navigation"]');
    if (!primarySidebar) {
      return false;
    }

    const primaryNav = primarySidebar.querySelector(".md-nav--primary");
    if (!primaryNav) {
      return false;
    }

    const activeTopLevelItem = getActiveTopLevelItem(primaryNav);
    if (!activeTopLevelItem) {
      return false;
    }

    // Case 1: active tab is a single page (for example Home/Quickstart/Troubleshooting).
    if (!activeTopLevelItem.classList.contains("md-nav__item--section")) {
      return true;
    }

    // Case 2: active section exists but only has one page, so sidebar adds little value.
    const sectionEntries = activeTopLevelItem.querySelectorAll(":scope > nav > .md-nav__list > .md-nav__item");
    return sectionEntries.length <= 1;
  }

  function applySidebarLayout() {
    document.body.classList.toggle(COLLAPSE_CLASS, shouldCollapsePrimarySidebar());
  }

  function scheduleApply() {
    if (scheduled) {
      return;
    }
    scheduled = true;
    window.requestAnimationFrame(function () {
      scheduled = false;
      applySidebarLayout();
    });
  }

  if (typeof window.document$ !== "undefined" && window.document$.subscribe) {
    window.document$.subscribe(scheduleApply);
  }

  document.addEventListener("DOMContentLoaded", function () {
    applySidebarLayout();
    const observer = new MutationObserver(scheduleApply);
    observer.observe(document.body, { childList: true, subtree: true });
  });
})();
