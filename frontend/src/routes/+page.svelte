<script lang="ts">
	import { onMount } from "svelte";
	import { goto } from "$app/navigation";
	import { refreshSession, startGoogleLogin, user } from "$lib/stores/auth";
	import Button from "$lib/components/ui/Button.svelte";
	import ThemeToggle from "$lib/components/ui/ThemeToggle.svelte";

	let busy = false;

	$: isAuthed = $user !== null;

	onMount(async () => {
		const session = await refreshSession();
		if (session) {
			goto("/dashboard");
		}
	});

	function handleLogin() {
		if (busy) return;
		busy = true;
		startGoogleLogin();
	}
</script>

<div class="login-page page">
	<div class="login-shell surface">
		<div class="login-left">
			<p class="eyebrow">HomeBound</p>
			<h1>Sign in to your personal workspace</h1>
			<p class="lead">
				A calm, focused space for apps, presence, and realtime chat. Built for a
				desktop-like workflow, tuned to Material You.
			</p>
			<div class="pill callout">
				Fast first render, minimal noise, and smooth transitions out of the box.
			</div>
		</div>
		<div class="login-right">
			<p class="section-title">Welcome back</p>
			<p class="subtext">Google sign-in only. Sessions expire automatically.</p>
			<button class="action-btn" on:click={handleLogin} disabled={busy}>
				Continue with Google
			</button>
			<Button variant="secondary" type="button">Learn about privacy</Button>
			<div class="login-actions">
				<ThemeToggle />
			</div>
		</div>
	</div>
</div>

<style>
	.login-page {
		padding: 6vh 8vw;
	}

	.login-shell {
		display: grid;
		grid-template-columns: minmax(240px, 1.2fr) minmax(240px, 0.8fr);
		gap: 48px;
		padding: 48px;
		align-items: center;
		width: min(1080px, 100%);
		margin: auto;
	}

	.login-left h1 {
		font-size: clamp(2rem, 3vw, 3rem);
		margin: 0 0 1rem 0;
	}

	.eyebrow {
		text-transform: uppercase;
		letter-spacing: 0.3em;
		font-size: 0.75rem;
		color: var(--md-sys-color-secondary);
		margin: 0 0 1rem 0;
	}

	.lead {
		font-size: 1.1rem;
		line-height: 1.6;
		color: var(--md-sys-color-on-surface-variant);
		margin: 0 0 2rem 0;
	}

	.callout {
		padding: 12px 20px;
		background: var(--md-sys-color-surface-container-high);
		color: var(--md-sys-color-on-surface-variant);
		font-weight: 500;
		width: fit-content;
	}

	.login-right {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 28px;
		background: var(--md-sys-color-surface-container-high);
		border-radius: 24px;
	}

	.login-actions {
		display: flex;
		justify-content: flex-end;
	}

	.subtext {
		color: var(--md-sys-color-on-surface-variant);
		margin: 0;
	}

	@media (max-width: 840px) {
		.login-page {
			padding: 4vh 6vw;
		}

		.login-shell {
			grid-template-columns: 1fr;
			padding: 32px;
		}
	}
</style>
