<template>
  <div class="upscale-codec">
    <v-select
      :disabled="props.disabled"
      label="Upscale Codec"
      v-model="selectCodec"
      variant="solo"
      :items="[
        {
          text: 'x265',
          value: 'libx265',
        },
        {
          text: 'av1',
          value: 'libsvtav1',
        },
      ]"
      item-title="text"
      item-value="value"
      hide-details
    ></v-select>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/tauri";

const props = defineProps<{
  disabled: boolean;
}>();

// The upscale codec. Default is `general`.
const selectCodec = ref("x265");

const emit = defineEmits(["upscale-codec-changed"]);

onMounted(async () => {
  try {
    const config = await invoke<{ ["default-upscale-codec"]: string }>(
      "load_configuration"
    );
    selectCodec.value = config["default-upscale-codec"];
  } catch (error: any) {
    await invoke("write_log", { message: error.toString() });
    alert(error);
  }
});

// Watch for the select between `general` and `digital` codec and sends selected codec to the parent component.
watch(selectCodec, (value) => {
  emit("upscale-codec-changed", value);
});
</script>

<style scoped lang="scss">
.upscale-codec {
  display: inline-block;
}
</style>
