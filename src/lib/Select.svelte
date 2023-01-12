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
    <div class="flex flex-row items-center gap-5">
        <Textfield class="flex-grow" bind:value label="Modpack Path"></Textfield>
        <Button on:click={select} disabled="{!valid}">Begin</Button>
    </div>
</div>
