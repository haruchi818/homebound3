<script lang="ts">
	import "../app.css";
	import { browser } from "$app/environment";
	import favicon from '$lib/assets/favicon.svg';
	import { theme } from "$lib/stores/theme";

	let { children } = $props();

	$effect(() => {
		if (browser) {
			document.documentElement.dataset.theme = $theme;
		}
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

{@render children()}
