<template>
    <manage-strategy
        v-for="(strat, key) in composedStrategies"
        :value="strat"
        @deleted="composedStrategies.splice(key, 1)"
    ></manage-strategy>

    <v-btn
        color="success"
        icon="mdi-plus"
        @click="addStrategyOverlay = !addStrategyOverlay"
    >
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
            composedStrategies: [
                {
                    "key": "run-shaper",
                    "name": "RunShaper",
                    "averageTime": 480,
                    "probability": 100,
                    "series": 1,
                    "strategies": [
                        {
                            "key": "run-shaper",
                            "name": "RunShaper",
                            "averageTime": 480,
                            "probability": 100,
                            "series": 1,
                            "strategies": []
                        }
                    ]
                }
            ]
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
