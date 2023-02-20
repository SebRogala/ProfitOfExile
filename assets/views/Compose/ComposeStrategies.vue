<template>
    <manage-strategy
        v-for="(strat, key) in composedStrategies"
        :value="strat"
        :available-strategies="availableStrategies"
        @deleted="composedStrategies.splice(key, 1)"
    ></manage-strategy>

    <v-btn
        color="success"
        @click="addStrategyOverlay = true"
    >
        Add strategy
    </v-btn>

    <add-strategy
        v-model:show-adder="addStrategyOverlay"
        :available-strategies="availableStrategies"
        @strategyAdded="addNewStrategy"
    ></add-strategy>

    <pre>{{ composedStrategies }}</pre>
</template>

<script>
import ManageStrategy from "./ManageStrategy";
import AddStrategy from "./AddStrategy";

export default {
    name: 'ComposeStrategies',
    components: {AddStrategy, ManageStrategy},
    data() {
        return {
            addStrategyOverlay: false,
            availableStrategies: [],
            composedStrategies: []
        }
    },
    mounted() {
        this.loadData();
    },
    methods: {
        loadData() {
            this.$api.get('/strategy/get-all').then(res => {
                this.availableStrategies = res.data;
            });
        },
        addNewStrategy(data) {
            this.composedStrategies.push(data);
        }
    }
}
</script>
