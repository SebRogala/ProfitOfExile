<template>
    <v-card
        class="mb-1"
        variant="elevated"
    >
        <v-card-title>
            {{ value.name }}
        </v-card-title>

        <v-card-text>
            <v-row>
                <v-col
                    cols="2"
                >
                    <v-text-field
                        label="Run times"
                        variant="outlined"
                        density="compact"
                        hide-details
                        v-model="value.series"
                    ></v-text-field>
                </v-col>
                <v-col
                    cols="2"
                >
                    <v-text-field
                        label="Average time to run (seconds)"
                        variant="outlined"
                        density="compact"
                        hide-details
                        v-model="value.averageTime"
                    ></v-text-field>
                </v-col>
                <v-col
                    cols="2"
                >
                    <v-text-field
                        label="Probability"
                        variant="outlined"
                        density="compact"
                        hide-details
                        v-model="value.probability"
                    ></v-text-field>
                </v-col>
            </v-row>


            <p v-if="value.strategies.length" class="text-h6 text-grey-darken-2 mt-2">Composed strategies:</p>

            <manage-strategy
                class="mt-3 ml-16"
                v-for="(strat, key) in value.strategies"
                :value="strat"
                :available-strategies="availableStrategies"
                @deleted="value.strategies.splice(key, 1)"
            ></manage-strategy>

        </v-card-text>

        <v-card-actions>
            <v-btn
                color="error"
                @click="$emit('deleted')"
            >Remove
            </v-btn>

            <v-btn
                color="success"
                @click="showAdder = true"
            >Add strategy
            </v-btn>
        </v-card-actions>

        <add-strategy
            v-model:show-adder="showAdder"
            :available-strategies="availableStrategies"
            @strategyAdded="addNewStrategy"
        ></add-strategy>
    </v-card>
</template>

<script>
import AddStrategy from "./AddStrategy";
export default {
    name: 'ManageStrategy',
    components: {AddStrategy},
    emits: ['deleted'],
    props: {
        value: Object,
        availableStrategies: Array
    },
    data() {
        return {
            showAdder: false,
        }
    },
    mounted() {
    },
    methods: {
        addNewStrategy(data) {
            this.value.strategies.push(data);
        }
    }
}
</script>
