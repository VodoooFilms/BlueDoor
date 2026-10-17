<script lang="ts">
  import Icon from "./Icon.svelte";
  let { kind }: { kind: "speech" | "privacy" | "speakers" } = $props();
</script>
<span class="model-icon" class:speech={kind === "speech"} class:privacy={kind === "privacy"} class:speakers={kind === "speakers"} aria-hidden="true">
  <Icon name={kind} size={30} />
</span>
<style>
.model-icon { display:grid; place-items:center; width:48px; height:48px; flex:none; border-radius:14px; border:1px solid currentColor; }
.speech { color:#8bf0d1; background:#0d827845; }
.privacy { color:#e0bbff; background:#985ce845; }
.speakers { color:#c5f583; background:#648b274d; }
</style>
