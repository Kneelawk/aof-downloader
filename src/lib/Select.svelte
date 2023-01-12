<script lang="ts">
    import {invoke} from "@tauri-apps/api/tauri";
    import Textfield from '@smui/textfield'
    import Button from '@smui/button'

    let value = ''
    $: valid = value.length != 0

    async function select() {
        await invoke('modpack_select', {value})
    }
</script>

<div>
    <div class="row">
        <Textfield bind:value label="Modpack Path" style="flex-grow: 1"></Textfield>
        <Button on:click={select} disabled="{!valid}">Begin</Button>
    </div>
</div>

<style>
    .row {
        gap: 10px;
        align-items: center;
    }
</style>
