(function () {
    var coal = document.getElementById("mdbook-theme-coal");
    if (coal) coal.textContent = "Dark";
    var light = document.getElementById("mdbook-theme-light");
    if (light) light.textContent = "Light";
    [
        "mdbook-theme-default_theme",
        "mdbook-theme-rust",
        "mdbook-theme-navy",
        "mdbook-theme-ayu",
    ].forEach(function (id) {
        var el = document.getElementById(id);
        if (el && el.parentElement) el.parentElement.remove();
    });
})();
