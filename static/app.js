(function () {
    const body = document.body;
    const status = document.getElementById("status");
    const form = document.getElementById("generateForm");
    const generateBtn = document.getElementById("generateBtn");
    const downloadAllBtn = document.getElementById("downloadAllBtn");
    const clearAllBtn = document.getElementById("clearAllBtn");
    const gallery = document.getElementById("gallery");
    const currentPreview = document.getElementById("currentPreview");
    const stageArt = document.getElementById("stageArt");
    const previewSize = document.getElementById("previewSize");

    function setStatus(text, isError) {
        status.textContent = text;
        status.classList.toggle("error", Boolean(isError));
    }

    function valueOf(id) {
        return document.getElementById(id).value;
    }

    function previewPath(set, background, size) {
        const params = new URLSearchParams({ set: set, background: background, size: String(size) });
        return "/preview?" + params.toString();
    }

    function trackPreviewErrors() {
        document.querySelectorAll(".tile img").forEach(function (image) {
            image.dataset.defaultAlt = image.alt;
            image.addEventListener("error", function () {
                const tile = image.closest(".tile");
                image.classList.add("unavailable");
                tile.classList.add("unavailable");
                tile.dataset.previewError = body.dataset.msgPreviewUnavailable;
            });
            image.addEventListener("load", function () {
                const tile = image.closest(".tile");
                image.classList.remove("unavailable");
                tile.classList.remove("unavailable");
                delete tile.dataset.previewError;
            });
        });
    }

    function updatePreviews() {
        const selectedSet = valueOf("set");
        const selectedBackground = valueOf("background");

        document.querySelectorAll("#setPreview [data-set]").forEach(function (tile) {
            tile.setAttribute("aria-pressed", String(tile.dataset.set === selectedSet));
        });

        document.querySelectorAll("#bgPreview [data-background]").forEach(function (tile) {
            tile.setAttribute("aria-pressed", String(tile.dataset.background === selectedBackground));
        });

        currentPreview.src = previewPath(selectedSet, selectedBackground, 300);
        previewSize.textContent = valueOf("size") + " × " + valueOf("size");
        stageArt.setAttribute("aria-busy", "true");
    }

    async function generate(event) {
        event.preventDefault();
        const count = parseInt(valueOf("count"), 10);
        const ttl = parseInt(valueOf("ttl_days"), 10);
        if (!Number.isFinite(count) || count < 1 || count > 50) {
            setStatus(body.dataset.msgInvalid, true);
            return;
        }
        const payload = {
            count: count,
            seed: valueOf("seed").trim() || null,
            set: valueOf("set"),
            background: valueOf("background"),
            size: parseInt(valueOf("size"), 10),
            ttl_days: Number.isFinite(ttl) ? ttl : 1,
        };

        generateBtn.disabled = true;
        generateBtn.setAttribute("aria-busy", "true");
        setStatus(body.dataset.msgGenerating, false);
        try {
            const response = await fetch("/generate", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload),
            });
            if (!response.ok) {
                let detail = body.dataset.msgError;
                try {
                    const problem = await response.json();
                    if (problem.error) detail = problem.error;
                } catch (ignore) {}
                throw new Error(detail);
            }
            setStatus(body.dataset.msgGenerated, false);
            window.location.reload();
        } catch (error) {
            setStatus(error.message || body.dataset.msgError, true);
            generateBtn.disabled = false;
            generateBtn.removeAttribute("aria-busy");
        }
    }

    async function removeAvatar(id) {
        if (!window.confirm(body.dataset.confirmDelete)) return;
        try {
            const response = await fetch("/avatar/" + encodeURIComponent(id), { method: "DELETE" });
            if (!response.ok) throw new Error(body.dataset.msgError);
            const card = gallery.querySelector('.avatar-card[data-id="' + CSS.escape(id) + '"]');
            if (card) card.remove();
            setStatus(body.dataset.msgDeleted, false);
            if (!gallery.querySelector(".avatar-card")) window.location.reload();
        } catch (error) {
            setStatus(error.message, true);
        }
    }

    async function clearAll() {
        if (!window.confirm(body.dataset.confirmClear)) return;
        try {
            const response = await fetch("/clear-all", { method: "POST" });
            if (!response.ok) throw new Error(body.dataset.msgError);
            setStatus(body.dataset.msgCleared, false);
            window.location.reload();
        } catch (error) {
            setStatus(error.message, true);
        }
    }

    currentPreview.addEventListener("load", function () {
        stageArt.setAttribute("aria-busy", "false");
        stageArt.classList.remove("unavailable");
    });
    currentPreview.addEventListener("error", function () {
        stageArt.setAttribute("aria-busy", "false");
        stageArt.classList.add("unavailable");
    });

    document.getElementById("size").addEventListener("change", updatePreviews);
    document.getElementById("setPreview").addEventListener("click", function (event) {
        const tile = event.target.closest("[data-set]");
        if (tile) {
            document.getElementById("set").value = tile.dataset.set;
            updatePreviews();
        }
    });
    document.getElementById("bgPreview").addEventListener("click", function (event) {
        const tile = event.target.closest("[data-background]");
        if (tile) {
            document.getElementById("background").value = tile.dataset.background;
            updatePreviews();
        }
    });
    trackPreviewErrors();
    updatePreviews();

    form.addEventListener("submit", generate);
    downloadAllBtn.addEventListener("click", function () {
        window.location.href = "/download-all";
    });
    clearAllBtn.addEventListener("click", clearAll);
    gallery.addEventListener("click", function (event) {
        const button = event.target.closest("[data-delete]");
        if (button) removeAvatar(button.dataset.delete);
    });
    document.querySelectorAll("[data-lang]").forEach(function (button) {
        button.addEventListener("click", function () {
            document.cookie = "lang=" + button.dataset.lang + "; Path=/; Max-Age=31536000";
            window.location.reload();
        });
    });
})();
