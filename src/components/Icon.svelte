<script lang="ts">
  let { name, size = 24 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    door: "M5 5 19 2v18L5 23V5Z",
    document: "M6 2h9l5 5v15H6V2Zm9 0v6h5M9 12h8M9 16h8",
    audio: "M3 11v2m4-6v10m5-15v20m5-15v10m4-6v2",
    edit: "m15 4 5 5M4 20l4-1L21 6l-5-5L3 14l1 6Z",
    settings: "m9 2-1 4-4 1-2 4 3 3-1 4 4 3 4-2 4 2 4-3-1-4 3-3-2-4-4-1-1-4H9ZM12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z",
    laptop: "M4 4h16v13H4V4ZM2 20h20l-2-3H4l-2 3Z",
    shield: "m12 2 9 4v6c0 5-9 10-9 10S3 17 3 12V6l9-4Zm-4 10 3 3 5-6",
    map: "m2 5 7-3 6 3 7-3v17l-7 3-6-3-7 3V5Zm7-3v17m6-14v17",
    globe: "M2 12h20M12 2c6 6 6 14 0 20-6-6-6-14 0-20ZM12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
    moon: "M21 14A9 9 0 0 1 10 3a9 9 0 1 0 11 11Z",
    expand: "M8 2H2v6m14-6h6v6M2 16v6h6m14-6v6h-6",
    info: "M12 11v6m0-10v1M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Z",
    download: "M12 2v14m-5-5 5 5 5-5M3 17v5h18v-5",
    plus: "M12 3v18M3 12h18", search: "M21 21l-6-6M10 2a8 8 0 1 0 0 16 8 8 0 0 0 0-16Z",
    check: "m4 12 5 5L21 5", close: "m5 5 14 14M5 19 19 5", mic: "M8 5a4 4 0 0 1 8 0v7a4 4 0 0 1-8 0V5ZM4 10v2a8 8 0 0 0 16 0v-2m-8 10v3m-4 0h8",
    upload: "M12 17V2m-6 6 6-6 6 6M3 15v7h18v-7", history: "M3 4v6h6M3 10a9 9 0 1 1 2 8m7-13v7l4 2",
  };
</script>
<svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] ?? paths.document} /></svg>
