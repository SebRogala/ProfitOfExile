<template>
    <v-timeline
        class="pb-1"
        density="compact"
        side="end"
    >
        <v-timeline-item
            v-for="(strategy, key) in value"
            size="small"
            :dot-color="colors[depth]"
        >
            <div class="text-h6">{{ strategy.name }}</div>

            <div class="d-flex justify-space-between flex-grow-1 pt-2 pb-3" style="max-width: 500px">
                <v-text-field
                    style="width: 150px"
                    class="mr-2"
                    label="Run times"
                    variant="outlined"
                    density="compact"
                    hide-details
                    v-model="strategy.series"
                ></v-text-field>

                <v-text-field
                    style="width: 150px"
                    class="mr-2"
                    label="Time to run (seconds)"
                    variant="outlined"
                    density="compact"
                    hide-details
                    v-model="strategy.averageTime"
                ></v-text-field>

                <v-text-field
                    style="width: 150px"
                    class="mr-2"
                    label="Probability"
                    variant="outlined"
                    density="compact"
                    hide-details
                    v-model="strategy.probability"
                ></v-text-field>

                <v-btn
                    color="error"
                    variant="flat"
                    icon="mdi-close"
                    size="x-small"
                    @click="emitDelete(key)"
                >
                </v-btn>
            </div>

            <manage-strategy
                class="ml-7 mb-4"
                :value="strategy.strategies"
                :available-strategies="availableStrategies"
                :parent-array-key="key"
                :depth="depth+1"
                @deleted="deleteStrategy"
            ></manage-strategy>

            <v-btn
                color="success"
                variant="outlined"
                @click="openAdder(key)"
            >Add strategy (to {{ strategy.name }})
            </v-btn>
        </v-timeline-item>

        <add-strategy
            v-model:show-adder="showAdder"
            :available-strategies="availableStrategies"
            @strategyAdded="addNewStrategy"
        ></add-strategy>
    </v-timeline>
</template>

<script>
import AddStrategy from "./AddStrategy";

export default {
    name: 'ManageStrategy',
    components: {AddStrategy},
    emits: ['deleted'],
    props: {
        value: Array,
        availableStrategies: Array,
        depth: Number,
        parentArrayKey: Number
    },
    data() {
        return {
            showAdder: false,
            toAddIndex: null,
            colors: [
                "#424242",
                "#F4511E",
                "#689F38",
                "#40C4FF",
                "#6A1B9A",
                "#1B5E20",
            ]
        }
    },
    mounted() {
    },
    methods: {
        addNewStrategy(data) {
            this.value[this.toAddIndex].strategies.push(data);
        },
        deleteStrategy(obj) {
            this.value[obj.parentKey].strategies.splice(obj.key, 1)
        },
        openAdder(key) {
            this.showAdder = true;
            this.toAddIndex = key;
        },
        emitDelete(key) {
            this.$emit('deleted', {key: key, parentKey: this.parentArrayKey})
        }
    }
}
</script>

<style>
.v-timeline-item__body {
    width: 100%;
}
</style>
