<template>
    <manage-strategy
        :value="composedStrategies"
        :available-strategies="availableStrategies"
        :depth="0"
        @deleted="deleteStrategy"
    ></manage-strategy>

    <div class="mt-3">
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
            Send
        </v-btn>

        <v-btn
            class="ml-16"
            color="warning"
            @click="composedStrategies = []"
        >
            Clear
        </v-btn>
    </div>

    <results
        :results="results"
    ></results>

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

export default {
    name: 'ComposeStrategies',
    components: {Results, AddStrategy, ManageStrategy},
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
        }
    }
}
</script>
