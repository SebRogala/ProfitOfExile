<template>
    <div class="mb-6 mt-4">
        <v-btn
            color="success"
            @click="addStrategyOverlay = true"
        >
            Add strategy
        </v-btn>

        <v-btn
            class="ml-4"
            color="info"
            @click="sendRequest"
        >
            Calculate
        </v-btn>

        <save-load-strategy
            :composedStrategy="composedStrategies"
            @loaded="strategyLoaded"
            @saved="clear"
        ></save-load-strategy>

        <v-btn
            class="ml-16"
            color="warning"
            @click="clear"
        >
            Clear
        </v-btn>
    </div>

    <results
        v-if="results.totalTimeInMinutes"
        :results="results"
    ></results>

    <manage-strategy
        :value="composedStrategies"
        :available-strategies="availableStrategies"
        :depth="0"
        @deleted="deleteStrategy"
    ></manage-strategy>

    <add-strategy
        v-model:show-adder="addStrategyOverlay"
        :available-strategies="availableStrategies"
        @strategyAdded="addNewStrategy"
    ></add-strategy>
</template>

<script>
import ManageStrategy from "./ManageStrategy";
import AddStrategy from "./AddStrategy";
import Results from "./Results";
import SaveLoadStrategy from "./SaveLoadStrategy";

export default {
    name: 'ComposeStrategies',
    components: {SaveLoadStrategy, Results, AddStrategy, ManageStrategy},
    data() {
        return {
            addStrategyOverlay: false,
            availableStrategies: [],
            composedStrategies: [],
            results: {}
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
        },
        deleteStrategy(obj) {
            this.composedStrategies.splice(obj.key, 1)
        },
        sendRequest() {
            this.$api.post('/strategy/compose', this.composedStrategies).then(res => {
                this.results = res.data;
            });
        },
        strategyLoaded(data) {
            this.composedStrategies = data;
        },
        clear() {
            this.composedStrategies = [];
            this.results = {}
        }
    }
}
</script>
