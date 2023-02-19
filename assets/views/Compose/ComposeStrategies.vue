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

    <v-overlay v-model="addStrategyOverlay" contained class="align-center justify-center">
        <div style="max-width: 600px" class="align-center justify-center">
            <v-btn
                v-for="strat in availableStrategies"
                class="ma-1"
                @click="addNewStrategy(strat)"
            >
                {{ strat.name }}
            </v-btn>
        </div>
    </v-overlay>

    <pre>{{ composedStrategies }}</pre>
</template>

<script>
import ManageStrategy from "./ManageStrategy";
export default {
    name: 'ComposeStrategies',
    components: {ManageStrategy},
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
        addNewStrategy(strat) {
            this.composedStrategies.push({
                "key": strat.key,
                "name": strat.name,
                "averageTime": strat.averageTime,
                "probability": strat.probability,
                "series": 1,
                "strategies": []
            });
            this.addStrategyOverlay = false;
        }
    }
}
</script>
