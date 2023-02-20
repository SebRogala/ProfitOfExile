<template>
    <v-timeline
        class="pb-1"
        density="compact"
        side="end"
    >
        <v-timeline-item
            v-for="(strategy, key) in value"
            size="small"
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
                    @click="$emit('deleted', key)"
                >
                </v-btn>
            </div>

            <v-btn
                color="success"
                variant="outlined"
                @click="openAdder(key)"
            >Add strategy (to {{ strategy.name }})
            </v-btn>

            <manage-strategy
                class="ml-7 mt-4"
                :value="strategy.strategies"
                :available-strategies="availableStrategies"
                @deleted="deleteStrategy"
            ></manage-strategy>
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
        availableStrategies: Array
    },
    data() {
        return {
            showAdder: false,
            toAddIndex: null,
        }
    },
    mounted() {
    },
    methods: {
        addNewStrategy(data) {
            this.value[this.toAddIndex].strategies.push(data);
        },
        deleteStrategy(key) {
            this.value.splice(key, 1)
        },
        openAdder(key) {
            this.showAdder = true;
            this.toAddIndex = key;
        }
    }
}
</script>

<style>
.v-timeline-item__body {
    width: 100%;
}
</style>
