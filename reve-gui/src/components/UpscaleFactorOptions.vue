<template>
  <div class="upscale-options">
    <v-select
      :disabled="props.disabled"
      label="Upscale Factor"
      v-model="selectFactor"
      variant="solo"
      :items="[
        {
          text: '2x',
          value: '2',
        },
        {
          text: '3x',
          value: '3',
        },
        {
          text: '4x',
          value: '4',
        },
      ]"
      item-title="text"
      item-value="value"
    ></v-select>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/tauri";

const props = defineProps<{
  disabled: boolean;
}>();

// The upscale type. Default is `general`.
const selectFactor = ref("2x");

const emit = defineEmits(["upscale-factor-changed"]);

onMounted(async () => {
  try {
    const config = await invoke<{ ["default-upscale-factor"]: string }>(
      "load_configuration"
    );
    selectFactor.value = config["default-upscale-factor"];
  } catch (error: any) {
    await invoke("write_log", { message: error.toString() });
    alert(error);
  }
});

// Watch for the select between `general` and `digital` type and sends selected type to the parent component.
watch(selectFactor, (value) => {
  emit("upscale-factor-changed", value);
});
</script>

<style scoped lang="scss">
.upscale-options {
  text-align: left;
  align-items: flex-start;
}
</style>
